use craft_core::EdgeNode;

pub struct EdgeRouteGenerator;

impl EdgeRouteGenerator {
    /// Generates a valid Velocity `velocity.toml` [servers] configuration snippet
    pub fn generate_velocity_config(
        cluster_name: &str,
        nodes: &[EdgeNode],
        backend_servers: &[(String, String, Option<String>)], // (name, endpoint, optional_region)
    ) -> String {
        let mut out = String::new();
        out.push_str("# -----------------------------------------------------------------------------\n");
        out.push_str(&format!("# Auto-Generated Craft Edge Routing: Cluster '{}'\n", cluster_name));
        out.push_str("# -----------------------------------------------------------------------------\n\n");

        out.push_str("[servers]\n");
        let mut server_names = Vec::new();

        for (name, endpoint, region) in backend_servers {
            server_names.push(format!("\"{}\"", name));
            let comment = if let Some(r) = region {
                format!(" # Region: {}", r)
            } else {
                String::new()
            };
            out.push_str(&format!("{} = \"{}\"{}\n", name, endpoint, comment));
        }

        out.push_str("\n[forced-hosts]\n");
        for node in nodes {
            if let Some(first) = backend_servers.first() {
                out.push_str(&format!(
                    "\"{}\" = [\"{}\"] # Edge node: {}\n",
                    node.endpoint, first.0, node.name
                ));
            }
        }

        out.push_str("\n# Fallback routing order:\n");
        out.push_str(&format!("try = [{}]\n", server_names.join(", ")));

        out
    }

    /// Generates a BungeeCord `config.yml` servers section snippet
    pub fn generate_bungeecord_config(
        nodes: &[EdgeNode],
        backend_servers: &[(String, String, Option<String>)],
    ) -> String {
        let mut out = String::new();
        out.push_str("# Auto-Generated Craft Edge Routing (BungeeCord)\n");
        out.push_str("servers:\n");

        for (name, endpoint, region) in backend_servers {
            let motd = if let Some(r) = region {
                format!("&1Craft Edge Node &8[&e{}&8]", r)
            } else {
                "&1Craft Dedicated Server".to_string()
            };
            out.push_str(&format!("  {}:\n", name));
            out.push_str(&format!("    address: \"{}\"\n", endpoint));
            out.push_str(&format!("    motd: \"{}\"\n", motd));
            out.push_str("    restricted: false\n");
        }

        out.push_str("# Registered Edge Ingress Nodes:\n");
        for node in nodes {
            out.push_str(&format!(
                "# Ingress: {} ({}) -> {}\n",
                node.name, node.region, node.endpoint
            ));
        }

        out
    }

    /// Generates an HAProxy TCP L4 stream configuration for low-level high-performance Anycast edge routing
    pub fn generate_haproxy_config(
        frontend_port: u16,
        nodes: &[EdgeNode],
        backend_servers: &[(String, String)],
    ) -> String {
        let mut out = String::new();
        out.push_str("# Auto-Generated Craft HAProxy L4 Edge Routing\n");
        out.push_str("global\n");
        out.push_str("    log stdout format raw local0\n");
        out.push_str("    maxconn 10000\n\n");

        out.push_str("defaults\n");
        out.push_str("    log global\n");
        out.push_str("    mode tcp\n");
        out.push_str("    option tcplog\n");
        out.push_str("    timeout connect 5000ms\n");
        out.push_str("    timeout client 300000ms\n");
        out.push_str("    timeout server 300000ms\n\n");

        out.push_str("frontend craft_edge_frontend\n");
        out.push_str(&format!("    bind *:{} mode tcp\n", frontend_port));
        out.push_str("    default_backend craft_edge_backends\n\n");

        out.push_str("backend craft_edge_backends\n");
        out.push_str("    mode tcp\n");
        out.push_str("    balance roundrobin\n");

        for (name, endpoint) in backend_servers {
            let weight = nodes
                .iter()
                .find(|n| n.name.eq_ignore_ascii_case(name))
                .map(|n| n.weight)
                .unwrap_or(100);
            out.push_str(&format!(
                "    server {} {} check fall 3 rise 2 weight {}\n",
                name, endpoint, weight
            ));
        }

        out
    }

    /// Generates Envoy static TCP proxy configuration snippet
    pub fn generate_envoy_tcp_config(
        frontend_port: u16,
        backend_servers: &[(String, String)],
    ) -> String {
        let mut out = String::new();
        out.push_str("# Auto-Generated Craft Envoy L4 TCP Configuration\n");
        out.push_str("static_resources:\n");
        out.push_str("  listeners:\n");
        out.push_str("  - name: edge_listener\n");
        out.push_str("    address:\n");
        out.push_str("      socket_address:\n");
        out.push_str("        address: 0.0.0.0\n");
        out.push_str(&format!("        port_value: {}\n", frontend_port));
        out.push_str("    filter_chains:\n");
        out.push_str("    - filters:\n");
        out.push_str("      - name: envoy.filters.network.tcp_proxy\n");
        out.push_str("        typed_config:\n");
        out.push_str("          \"@type\": type.googleapis.com/envoy.extensions.filters.network.tcp_proxy.v3.TcpProxy\n");
        out.push_str("          stat_prefix: craft_edge_tcp\n");
        out.push_str("          cluster: backend_cluster\n\n");

        out.push_str("  clusters:\n");
        out.push_str("  - name: backend_cluster\n");
        out.push_str("    connect_timeout: 0.25s\n");
        out.push_str("    type: STRICT_DNS\n");
        out.push_str("    lb_policy: ROUND_ROBIN\n");
        out.push_str("    load_assignment:\n");
        out.push_str("      cluster_name: backend_cluster\n");
        out.push_str("      endpoints:\n");
        out.push_str("      - lb_endpoints:\n");

        for (_name, endpoint) in backend_servers {
            let parts: Vec<&str> = endpoint.split(':').collect();
            let host = parts.first().unwrap_or(&"127.0.0.1");
            let port = parts.get(1).unwrap_or(&"25565");

            out.push_str("        - endpoint:\n");
            out.push_str("            address:\n");
            out.push_str("              socket_address:\n");
            out.push_str(&format!("                address: {}\n", host));
            out.push_str(&format!("                port_value: {}\n", port));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_velocity_config_generation() {
        let node1 = EdgeNode::new("edge-us-east", "us-east", "us-east.craft.internal:25565");
        let nodes = vec![node1];

        let servers = vec![
            ("lobby-1".to_string(), "10.0.1.10:25565".to_string(), Some("us-east".to_string())),
            ("survival-1".to_string(), "10.0.1.20:25565".to_string(), Some("us-east".to_string())),
        ];

        let config = EdgeRouteGenerator::generate_velocity_config("global-prod", &nodes, &servers);
        assert!(config.contains("[servers]"));
        assert!(config.contains("lobby-1 = \"10.0.1.10:25565\""));
        assert!(config.contains("survival-1 = \"10.0.1.20:25565\""));
        assert!(config.contains("try = [\"lobby-1\", \"survival-1\"]"));
        assert!(config.contains("\"us-east.craft.internal:25565\" = [\"lobby-1\"]"));
    }

    #[test]
    fn test_haproxy_config_generation() {
        let mut node1 = EdgeNode::new("backend-1", "us-east", "10.0.1.10:25565");
        node1.weight = 80;
        let nodes = vec![node1];

        let servers = vec![("backend-1".to_string(), "10.0.1.10:25565".to_string())];
        let config = EdgeRouteGenerator::generate_haproxy_config(25565, &nodes, &servers);

        assert!(config.contains("frontend craft_edge_frontend"));
        assert!(config.contains("bind *:25565 mode tcp"));
        assert!(config.contains("server backend-1 10.0.1.10:25565 check fall 3 rise 2 weight 80"));
    }

    #[test]
    fn test_bungeecord_config_generation() {
        let nodes = vec![EdgeNode::new("edge-1", "eu-central", "10.0.0.1:25565")];
        let servers = vec![("hub".to_string(), "10.0.0.10:25565".to_string(), Some("eu-central".to_string()))];
        let config = EdgeRouteGenerator::generate_bungeecord_config(&nodes, &servers);

        assert!(config.contains("servers:"));
        assert!(config.contains("hub:"));
        assert!(config.contains("address: \"10.0.0.10:25565\""));
    }
}
