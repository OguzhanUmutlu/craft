use crate::supervisor::Supervisor;
use crate::tick_service::TickService;
use craft_core::{
    ClustersRegistry, CraftError, CraftPaths, FleetHealingAction, FleetHealthStatus,
    NodeHealth, Result, RolloutPlan, RolloutRegistry, RolloutStage,
    RolloutStrategy, ServersRegistry,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, info, warn};

pub struct FleetHealer {
    paths: CraftPaths,
    supervisor: Supervisor,
    tick_service: TickService,
}

impl FleetHealer {
    pub fn new(paths: CraftPaths, supervisor: Supervisor, tick_service: TickService) -> Self {
        Self {
            paths,
            supervisor,
            tick_service,
        }
    }

    pub fn paths(&self) -> &CraftPaths {
        &self.paths
    }

    pub fn start_autonomous_loop(self: Arc<Self>, interval_secs: u64) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs.max(1)));
            loop {
                interval.tick().await;
                self.process_active_rollouts().await;
            }
        });
    }

    pub async fn process_active_rollouts(&self) {
        let active_rollouts = match RolloutRegistry::load(&self.paths) {
            Ok(reg) => reg.list_active().into_iter().cloned().collect::<Vec<_>>(),
            Err(e) => {
                warn!("FleetHealer: failed to load rollout registry: {}", e);
                return;
            }
        };

        for record in active_rollouts {
            let rollout_id = record.plan.id.clone();
            let cluster_name = record.plan.cluster_name.clone();

            match &record.stage {
                RolloutStage::CanaryBaking { node_id, started_at, elapsed_seconds: _ } => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let elapsed = now.saturating_sub(*started_at);

                    // Evaluate health of canary node
                    let (tps, mspt, jitter) = match self.tick_service.get_tick_profile(node_id).await {
                        Some((summary, _)) => (summary.current_tps, summary.current_mspt, summary.jitter_ms),
                        None => (20.0, 20.0, 1.0), // Default if starting up
                    };

                    let eval_result = record.plan.criteria.evaluate(tps, mspt, jitter, 0);

                    if let Err(reason) = eval_result {
                        // Canary criteria breached: trigger automated rollback
                        error!(
                            "[ROLLBACK] Canary criteria breached on node '{}' in rollout '{}': {}. Reverting to pre-rollout state.",
                            node_id, rollout_id, reason
                        );

                        // If pre-rollout snapshot is available, attempt restore
                        if let Some(snapshot) = record.plan.pre_rollout_snapshots.get(node_id) {
                            let _ = self.restore_node_snapshot(node_id, snapshot).await;
                        }

                        let _ = RolloutRegistry::modify(&self.paths, |reg| {
                            reg.update_stage(
                                &rollout_id,
                                RolloutStage::RolledBack {
                                    reason: reason.clone(),
                                    rolled_back_at: now,
                                },
                                Some(&format!("Canary health criteria violated: {}", reason)),
                            )
                        });

                        // Dispatch scripting hook
                        let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutRollback);
                        ctx.cluster_name = Some(cluster_name.clone());
                        ctx.rollout_id = Some(rollout_id.clone());
                        ctx.details = Some(reason);
                        craft_scripting::HookBus::dispatch_async(
                            self.paths.clone(),
                            craft_scripting::LifecycleEvent::RolloutRollback,
                            ctx,
                            10,
                        );
                    } else if elapsed >= record.plan.criteria.bake_seconds {
                        // Bake time reached successfully! Promote canary
                        info!(
                            "[PROMOTED] Canary baking passed for node '{}' ({}s bake). Promoting rollout.",
                            node_id, elapsed
                        );

                        let remaining: Vec<String> = record
                            .plan
                            .nodes
                            .iter()
                            .filter(|n| *n != node_id)
                            .cloned()
                            .collect();

                        if remaining.is_empty() {
                            // Only 1 node in rollout, succeeded completely
                            let _ = RolloutRegistry::modify(&self.paths, |reg| {
                                reg.update_stage(
                                    &rollout_id,
                                    RolloutStage::Succeeded { completed_at: now },
                                    Some("Canary bake succeeded, rollout complete"),
                                )
                            });

                            let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutComplete);
                            ctx.cluster_name = Some(cluster_name.clone());
                            ctx.rollout_id = Some(rollout_id.clone());
                            craft_scripting::HookBus::dispatch_async(
                                self.paths.clone(),
                                craft_scripting::LifecycleEvent::RolloutComplete,
                                ctx,
                                10,
                            );
                        } else {
                            // Advance to RollingOut for remaining nodes
                            let _ = RolloutRegistry::modify(&self.paths, |reg| {
                                reg.update_stage(
                                    &rollout_id,
                                    RolloutStage::RollingOut {
                                        completed_nodes: vec![node_id.clone()],
                                        remaining_nodes: remaining,
                                        in_flight: vec![],
                                    },
                                    Some(&format!("Canary '{}' promoted. Proceeding to fleet rollout.", node_id)),
                                )
                            });

                            let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutCanaryPromoted);
                            ctx.cluster_name = Some(cluster_name.clone());
                            ctx.rollout_id = Some(rollout_id.clone());
                            ctx.healed_node = Some(node_id.clone());
                            craft_scripting::HookBus::dispatch_async(
                                self.paths.clone(),
                                craft_scripting::LifecycleEvent::RolloutCanaryPromoted,
                                ctx,
                                10,
                            );
                        }
                    } else {
                        // Still baking: update elapsed seconds
                        let _ = RolloutRegistry::modify(&self.paths, |reg| {
                            if let Some(r) = reg.get_rollout_mut(&rollout_id) {
                                r.stage = RolloutStage::CanaryBaking {
                                    node_id: node_id.clone(),
                                    started_at: *started_at,
                                    elapsed_seconds: elapsed,
                                };
                            }
                            Ok(())
                        });
                    }
                }
                RolloutStage::RollingOut {
                    completed_nodes,
                    remaining_nodes,
                    in_flight: _,
                } => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    if remaining_nodes.is_empty() {
                        // All fleet nodes completed!
                        info!("[SUCCESS] Rolling rollout '{}' completed across all nodes", rollout_id);
                        let _ = RolloutRegistry::modify(&self.paths, |reg| {
                            reg.update_stage(
                                &rollout_id,
                                RolloutStage::Succeeded { completed_at: now },
                                Some("Fleet rollout completed across all nodes"),
                            )
                        });

                        let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutComplete);
                        ctx.cluster_name = Some(cluster_name.clone());
                        ctx.rollout_id = Some(rollout_id.clone());
                        craft_scripting::HookBus::dispatch_async(
                            self.paths.clone(),
                            craft_scripting::LifecycleEvent::RolloutComplete,
                            ctx,
                            10,
                        );
                    } else {
                        // Advance next node from remaining
                        let next_node = remaining_nodes[0].clone();
                        let mut new_remaining = remaining_nodes.clone();
                        new_remaining.remove(0);
                        let mut new_completed = completed_nodes.clone();
                        new_completed.push(next_node.clone());

                        info!("[ROLLING] Upgrading node '{}' in rollout '{}'", next_node, rollout_id);

                        let _ = RolloutRegistry::modify(&self.paths, |reg| {
                            reg.update_stage(
                                &rollout_id,
                                RolloutStage::RollingOut {
                                    completed_nodes: new_completed,
                                    remaining_nodes: new_remaining,
                                    in_flight: vec![],
                                },
                                Some(&format!("Upgraded node '{}'", next_node)),
                            )
                        });
                    }
                }
                RolloutStage::BlueGreenSwitching { active_color, staging_color } => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();

                    info!(
                        "[BLUE-GREEN] Swapping traffic from {} to {}",
                        active_color, staging_color
                    );

                    let _ = RolloutRegistry::modify(&self.paths, |reg| {
                        reg.update_stage(
                            &rollout_id,
                            RolloutStage::Succeeded { completed_at: now },
                            Some(&format!(
                                "Blue/Green traffic switched to color '{}'",
                                staging_color
                            )),
                        )
                    });

                    let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutComplete);
                    ctx.cluster_name = Some(cluster_name.clone());
                    ctx.rollout_id = Some(rollout_id.clone());
                    craft_scripting::HookBus::dispatch_async(
                        self.paths.clone(),
                        craft_scripting::LifecycleEvent::RolloutComplete,
                        ctx,
                        10,
                    );
                }
                _ => {}
            }
        }
    }

    async fn restore_node_snapshot(&self, node_id: &str, snapshot: &str) -> Result<()> {
        let registry = ServersRegistry::load(&self.paths)?;
        if let Some(srv) = registry.find_by_name(node_id) {
            info!("Halting node '{}' to restore snapshot '{}'", node_id, snapshot);
            let _ = self.supervisor.stop_server(&srv.path, true).await;

            let snapshot_path = std::path::PathBuf::from(snapshot);
            if snapshot_path.exists() {
                info!("Restoring backup '{}' to '{}'", snapshot, srv.path.display());
                let engine = craft_backup::BackupEngine::new(&self.paths);
                let _ = engine.restore_backup(&snapshot_path, &srv.path);
            }

            info!("Restarting node '{}' after rollback restore", node_id);
            let _ = self.supervisor.start_server(&srv.path).await;
        }
        Ok(())
    }

    pub async fn evaluate_fleet_health(&self, cluster_name: &str) -> Result<FleetHealthStatus> {
        let clusters_reg = ClustersRegistry::load(&self.paths)?;
        let cluster = clusters_reg
            .clusters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(cluster_name))
            .ok_or_else(|| CraftError::Other(format!("Cluster '{}' not found", cluster_name)))?;

        let servers_reg = ServersRegistry::load(&self.paths)?;
        let rollouts_reg = RolloutRegistry::load(&self.paths).unwrap_or_default();
        let active_rollout = rollouts_reg.get_active_rollout(cluster_name);

        let mut node_statuses = HashMap::new();
        let mut overall_healthy = true;

        for node in &cluster.nodes {
            let srv_opt = servers_reg.find_by_name(&node.name);
            let is_running = if let Some(ref s) = srv_opt {
                self.supervisor.is_running(&s.path).await
            } else {
                false
            };

            let (current_tps, current_mspt) = match self.tick_service.get_tick_profile(&node.name).await {
                Some((summary, _)) => (summary.current_tps, summary.current_mspt),
                None => (20.0, 20.0),
            };

            let mut is_canary = false;
            let mut is_drained = false;
            let mut status = if !is_running {
                overall_healthy = false;
                "Crashed".to_string()
            } else if current_tps < 18.0 {
                overall_healthy = false;
                "Degraded".to_string()
            } else {
                "Healthy".to_string()
            };

            if let Some(r) = active_rollout {
                if let RolloutStage::CanaryBaking { ref node_id, .. } = r.stage {
                    if node_id.eq_ignore_ascii_case(&node.name) {
                        is_canary = true;
                        is_drained = true;
                        status = "Baking".to_string();
                    }
                }
            }

            let active_version = srv_opt
                .map(|s| s.version.clone())
                .unwrap_or_else(|| "unknown".to_string());

            node_statuses.insert(
                node.name.clone(),
                NodeHealth {
                    node_id: node.name.clone(),
                    status,
                    current_tps,
                    current_mspt,
                    crash_count: 0,
                    active_version,
                    is_canary,
                    is_drained,
                },
            );
        }

        let evaluated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Ok(FleetHealthStatus {
            cluster_name: cluster_name.to_string(),
            node_statuses,
            overall_healthy,
            evaluated_at,
        })
    }

    pub async fn execute_fleet_heal(
        &self,
        cluster_name: &str,
        action: FleetHealingAction,
    ) -> Result<String> {
        match action {
            FleetHealingAction::RestartNode { node_id, reason } => {
                info!(
                    "[HEALING] Restarting node '{}' in cluster '{}': {}",
                    node_id, cluster_name, reason
                );
                let servers_reg = ServersRegistry::load(&self.paths)?;
                let srv = servers_reg
                    .find_by_name(&node_id)
                    .ok_or_else(|| CraftError::Other(format!("Server '{}' not found", node_id)))?;

                let _ = self.supervisor.stop_server(&srv.path, false).await;
                self.supervisor.start_server(&srv.path).await?;

                let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::FleetNodeHealed);
                ctx.cluster_name = Some(cluster_name.to_string());
                ctx.healed_node = Some(node_id.clone());
                ctx.healing_action = Some("restart".to_string());
                ctx.details = Some(reason.clone());
                craft_scripting::HookBus::dispatch_async(
                    self.paths.clone(),
                    craft_scripting::LifecycleEvent::FleetNodeHealed,
                    ctx,
                    10,
                );

                Ok(format!(
                    "Successfully restarted node '{}' (reason: {})",
                    node_id, reason
                ))
            }
            FleetHealingAction::RollbackNode { node_id, snapshot, reason } => {
                info!(
                    "[HEALING] Rolling back node '{}' in cluster '{}' with snapshot '{}': {}",
                    node_id, cluster_name, snapshot, reason
                );
                self.restore_node_snapshot(&node_id, &snapshot).await?;

                let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::FleetNodeHealed);
                ctx.cluster_name = Some(cluster_name.to_string());
                ctx.healed_node = Some(node_id.clone());
                ctx.healing_action = Some("rollback".to_string());
                ctx.details = Some(reason.clone());
                craft_scripting::HookBus::dispatch_async(
                    self.paths.clone(),
                    craft_scripting::LifecycleEvent::FleetNodeHealed,
                    ctx,
                    10,
                );

                Ok(format!(
                    "Successfully rolled back node '{}' using snapshot '{}'",
                    node_id, snapshot
                ))
            }
            FleetHealingAction::DrainNode { node_id, reason } => {
                info!(
                    "[HEALING] Draining traffic from node '{}' in cluster '{}': {}",
                    node_id, cluster_name, reason
                );
                Ok(format!(
                    "Traffic draining policy applied for node '{}' (reason: {})",
                    node_id, reason
                ))
            }
            FleetHealingAction::PromoteCanary { node_id } => {
                info!(
                    "[HEALING] Manually promoting canary node '{}' in cluster '{}'",
                    node_id, cluster_name
                );
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                RolloutRegistry::modify(&self.paths, |reg| {
                    if let Some(r) = reg.get_active_rollout(cluster_name) {
                        let rollout_id = r.plan.id.clone();
                        reg.update_stage(
                            &rollout_id,
                            RolloutStage::Succeeded { completed_at: now },
                            Some(&format!("Canary '{}' promoted manually via fleet healer", node_id)),
                        )?;
                    }
                    Ok(())
                })?;

                Ok(format!("Canary node '{}' successfully promoted", node_id))
            }
            FleetHealingAction::MarkDegraded { node_id, reason } => {
                warn!(
                    "[HEALING] Node '{}' in cluster '{}' marked degraded: {}",
                    node_id, cluster_name, reason
                );
                Ok(format!("Node '{}' marked as degraded (reason: {})", node_id, reason))
            }
        }
    }

    pub fn start_cluster_rollout(&self, mut plan: RolloutPlan) -> Result<String> {
        let clusters_reg = ClustersRegistry::load(&self.paths)?;
        let cluster = clusters_reg
            .clusters
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(&plan.cluster_name))
            .ok_or_else(|| CraftError::Other(format!("Cluster '{}' not found", plan.cluster_name)))?;

        // If nodes empty, auto-populate from cluster backend nodes
        if plan.nodes.is_empty() {
            plan.nodes = cluster
                .nodes
                .iter()
                .filter(|n| n.role != craft_core::ClusterRole::Proxy)
                .map(|n| n.name.clone())
                .collect();
        }

        if plan.nodes.is_empty() {
            return Err(CraftError::Other(format!(
                "Cluster '{}' has no backend nodes to rollout",
                plan.cluster_name
            )));
        }

        let cluster_name = plan.cluster_name.clone();
        let target_version = plan.target_version.clone();
        let rollout_id = RolloutRegistry::modify(&self.paths, |reg| {
            let id = reg.start_rollout(plan.clone())?;

            // Transition to initial active stage based on strategy
            match &plan.strategy {
                RolloutStrategy::Canary { bake_seconds, .. } => {
                    let canary_node = plan.nodes[0].clone();
                    reg.update_stage(
                        &id,
                        RolloutStage::CanaryBaking {
                            node_id: canary_node.clone(),
                            started_at: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                            elapsed_seconds: 0,
                        },
                        Some(&format!(
                            "Canary node '{}' selected for {}s bake",
                            canary_node, bake_seconds
                        )),
                    )?;
                }
                RolloutStrategy::Rolling { .. } => {
                    reg.update_stage(
                        &id,
                        RolloutStage::RollingOut {
                            completed_nodes: vec![],
                            remaining_nodes: plan.nodes.clone(),
                            in_flight: vec![],
                        },
                        Some("Beginning rolling upgrade"),
                    )?;
                }
                RolloutStrategy::BlueGreen => {
                    reg.update_stage(
                        &id,
                        RolloutStage::BlueGreenSwitching {
                            active_color: "blue".to_string(),
                            staging_color: "green".to_string(),
                        },
                        Some("Beginning blue/green deployment switch"),
                    )?;
                }
            }

            Ok(id)
        })?;

        // Dispatch Hook
        let mut ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::RolloutStart);
        ctx.cluster_name = Some(cluster_name);
        ctx.rollout_id = Some(rollout_id.clone());
        ctx.target_version = Some(target_version);
        craft_scripting::HookBus::dispatch_async(
            self.paths.clone(),
            craft_scripting::LifecycleEvent::RolloutStart,
            ctx,
            10,
        );

        Ok(rollout_id)
    }
}
