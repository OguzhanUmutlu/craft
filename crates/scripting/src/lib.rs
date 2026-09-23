pub mod config;
pub mod definition;
pub mod engine;
pub mod hooks;
pub mod package;
pub mod properties_schema;
pub mod starter;

pub use config::{CustomRuntimeType, CustomServerConfig, CUSTOM_CONFIG_FILE};
pub use definition::*;
pub use engine::LuaEngine;
pub use hooks::*;
pub use mlua;
pub use package::*;
pub use properties_schema::*;
pub use starter::*;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lua_engine_basic() {
        let engine = LuaEngine::new().expect("engine should initialize");
        let result: i32 = engine
            .eval("return 10 + 20")
            .expect("evaluation should succeed");
        assert_eq!(result, 30);

        let str_result: String = engine
            .eval("return 'hello ' .. 'world'")
            .expect("string concat should succeed");
        assert_eq!(str_result, "hello world");
    }

    #[test]
    fn test_craft_api_injection() {
        let engine = LuaEngine::new().expect("engine should initialize");

        // craft.version
        let version: String = engine
            .eval("return craft.version")
            .expect("craft.version should exist");
        assert!(!version.is_empty());

        // craft.time and craft.time_ms
        let time: u64 = engine
            .eval("return craft.time()")
            .expect("craft.time() should return timestamp");
        assert!(time > 0);

        // craft.sleep
        engine
            .exec("craft.sleep(5)")
            .expect("craft.sleep should succeed");

        // craft.env
        engine
            .exec("craft.env('CRAFT_TEST_LUA_KEY', 'test_val_123')")
            .expect("craft.env set should succeed");
        let env_val: String = engine
            .eval("return craft.env('CRAFT_TEST_LUA_KEY')")
            .expect("craft.env get should succeed");
        assert_eq!(env_val, "test_val_123");

        // craft.log, craft.info, craft.warn, craft.error
        engine
            .exec("craft.log('test log message')")
            .expect("craft.log should succeed");
        engine
            .exec("craft.info('test info message')")
            .expect("craft.info should succeed");
        engine
            .exec("craft.warn('test warn message')")
            .expect("craft.warn should succeed");
        engine
            .exec("craft.error('test error message')")
            .expect("craft.error should succeed");
    }

    #[test]
    fn test_craft_fs_api() {
        let dir = tempdir().expect("tempdir");
        let test_file = dir.path().join("test_lua_fs.txt");
        let test_path_str = test_file.to_string_lossy().replace('\\', "/");

        let engine = LuaEngine::new().expect("engine should initialize");

        // Write file
        let write_script = format!("craft.fs.write('{}', 'hello craft fs')", test_path_str);
        engine.exec(&write_script).expect("write should succeed");

        // Exists & is_file
        let exists: bool = engine
            .eval(&format!("return craft.fs.exists('{}')", test_path_str))
            .expect("exists check");
        assert!(exists);
        let is_file: bool = engine
            .eval(&format!("return craft.fs.is_file('{}')", test_path_str))
            .expect("is_file check");
        assert!(is_file);

        // Read file
        let content: String = engine
            .eval(&format!("return craft.fs.read('{}')", test_path_str))
            .expect("read check");
        assert_eq!(content, "hello craft fs");

        // Append to file
        let append_script = format!("craft.fs.append('{}', '\\nsecond line')", test_path_str);
        engine.exec(&append_script).expect("append should succeed");
        let appended_content: String = engine
            .eval(&format!("return craft.fs.read('{}')", test_path_str))
            .expect("read check after append");
        assert!(appended_content.contains("second line"));
    }

    #[test]
    fn test_craft_exec_api() {
        let engine = LuaEngine::new().expect("engine should initialize");
        #[cfg(unix)]
        {
            let res: bool = engine
                .eval("return craft.exec('echo', {'test_output'}).success")
                .expect("exec should succeed");
            assert!(res);
            let out: String = engine
                .eval("return craft.exec('echo', {'test_output'}).stdout")
                .expect("stdout should be returned");
            assert!(out.contains("test_output"));
        }
        #[cfg(windows)]
        {
            let res: bool = engine
                .eval("return craft.exec('cmd', {'/C', 'echo test_output'}).success")
                .expect("exec should succeed");
            assert!(res);
        }
    }

    #[test]
    fn test_custom_server_context_and_hooks() {
        let dir = tempdir().expect("tempdir");
        let server_dir = dir.path();

        let config = CustomServerConfig::default_lua("test-server", 8080);
        config.save_to_dir(server_dir).expect("save config");

        let hooks_code = r#"
            function on_pre_start(server)
                craft.fs.write(craft.server.path .. "/pre_start.txt", "pre_start:" .. server.name)
            end

            function on_post_start(server, pid)
                craft.fs.write(craft.server.path .. "/post_start.txt", "post_start:" .. tostring(pid))
            end

            function on_pre_stop(server, pid)
                craft.fs.write(craft.server.path .. "/pre_stop.txt", "pre_stop:" .. tostring(pid))
            end

            function on_post_stop(server, exit_code)
                craft.fs.write(craft.server.path .. "/post_stop.txt", "post_stop:" .. tostring(exit_code))
            end

            function health_check(server)
                return true
            end
        "#;
        std::fs::write(server_dir.join("hooks.lua"), hooks_code).expect("write hooks");

        // Run hooks
        LuaEngine::run_pre_start(server_dir, &config).expect("run_pre_start");
        assert!(server_dir.join("pre_start.txt").exists());
        let pre_content = std::fs::read_to_string(server_dir.join("pre_start.txt")).unwrap();
        assert_eq!(pre_content, "pre_start:test-server");

        LuaEngine::run_post_start(server_dir, &config, 12345).expect("run_post_start");
        assert!(server_dir.join("post_start.txt").exists());
        let post_content = std::fs::read_to_string(server_dir.join("post_start.txt")).unwrap();
        assert_eq!(post_content, "post_start:12345");

        LuaEngine::run_pre_stop(server_dir, &config, 12345).expect("run_pre_stop");
        assert!(server_dir.join("pre_stop.txt").exists());

        LuaEngine::run_post_stop(server_dir, &config, 0).expect("run_post_stop");
        assert!(server_dir.join("post_stop.txt").exists());

        let healthy = LuaEngine::run_health_check(server_dir, &config).expect("run_health_check");
        assert!(healthy);
    }

    #[test]
    fn test_custom_config_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let config = CustomServerConfig::default_binary("binary-srv", 25565, "./bin/server");
        config.save_to_dir(dir.path()).expect("save config");

        let loaded = CustomServerConfig::load_from_dir(dir.path())
            .expect("load config")
            .expect("config exists");
        assert_eq!(loaded.server.name, "binary-srv");
        assert_eq!(loaded.server.runtime_type, CustomRuntimeType::Binary);
        assert_eq!(loaded.network.port, 25565);
        assert_eq!(loaded.runtime.executable, "./bin/server");
    }

    #[test]
    fn test_starter_generation() {
        let dir = tempdir().expect("tempdir");
        let config = CustomServerConfig::default_lua("lua-game", 9000);
        generate_all_starter_files(dir.path(), &config).expect("starter generation");

        assert!(dir.path().join("craft.custom.toml").exists());
        assert!(dir.path().join("server.lua").exists());
        assert!(dir.path().join("hooks.lua").exists());
        assert!(dir.path().join("start.sh").exists());
        assert!(dir.path().join("start.cmd").exists());
        assert!(dir.path().join("README.md").exists());
    }

    #[test]
    fn test_run_file_with_args() {
        let dir = tempdir().expect("tempdir");
        let script_path = dir.path().join("test_args.lua");
        let output_path = dir.path().join("output.txt");
        let output_path_str = output_path.to_string_lossy().replace('\\', "/");

        let script = format!(
            r#"
            local arg1 = arg[1] or "missing"
            local arg2 = arg[2] or "missing"
            craft.fs.write('{}', arg1 .. ":" .. arg2)
        "#,
            output_path_str
        );
        std::fs::write(&script_path, script).expect("write script");

        let engine = LuaEngine::new().expect("engine");
        let args = vec!["apple".to_string(), "banana".to_string()];
        engine.run_file(&script_path, &args).expect("run_file");

        let content = std::fs::read_to_string(&output_path).expect("read output");
        assert_eq!(content, "apple:banana");
    }

    #[test]
    fn test_software_definition_roundtrip() {
        let toml_content = r#"
[software]
id = "test-game"
name = "Test Game Server"
display_name = "Test Game (Dedicated)"
game = "testgame"
edition = "native"
description = "A great test game server"
version = "1.0.0"

[capabilities]
plugins = true
mods = false
datapacks = false
rcon = true

[runtime]
kind = "native"
default_server_file = "test_server"
arguments = ["--port", "{port}"]
stop_method = "sigterm"
stop_timeout_seconds = 10

[network]
default_port = 7777
protocol = "udp"

[versions]
recommended = "2.0.0"
bundled = ["2.0.0", "1.9.0"]
fetch_mode = "static"

[assets]
download_mode = "url_template"
url_template = "https://example.com/downloads/v{version}/server.tar.gz"
filename = "server.tar.gz"
is_archive = true
strip_components = 1

[properties]
file = "config.toml"
format = "toml"
schema_file = "properties.toml"

[developer]
reload_command = "reload"
"#;

        let def = SoftwareDefinition::parse(toml_content).expect("parse definition");
        assert_eq!(def.id(), "test-game");
        assert_eq!(def.name(), "Test Game Server");
        assert_eq!(def.display_name(), "Test Game (Dedicated)");
        assert_eq!(def.game(), "testgame");
        assert_eq!(def.edition(), "native");
        assert!(def.capabilities.plugins);
        assert!(!def.capabilities.mods);
        assert_eq!(def.network.default_port, 7777);
        assert_eq!(def.network.protocol, "udp");
        assert_eq!(def.default_server_file(), "test_server");
        assert_eq!(def.versions.bundled.len(), 2);

        let serialized = def.to_toml().expect("serialize definition");
        let def2 = SoftwareDefinition::parse(&serialized).expect("reparse definition");
        assert_eq!(def, def2);
    }

    #[test]
    fn test_properties_schema_and_generic_properties() {
        let schema_toml = r#"
[meta]
file = "server.properties"
format = "properties"

[[categories]]
id = "general"
name = "General Settings"

[[categories]]
id = "network"
name = "Network Settings"

[[properties]]
key = "server-port"
category = "network"
label = "Server Port"
description = "Port to listen on"
type = "integer"
default = 25565
min = 1
max = 65535

[[properties]]
key = "online-mode"
category = "general"
label = "Online Mode"
type = "boolean"
default = true

[[properties]]
key = "difficulty"
category = "general"
label = "Difficulty"
type = "enum"
options = ["peaceful", "easy", "normal", "hard"]
default = "normal"
"#;

        let schema = PropertiesSchema::parse(schema_toml).expect("parse schema");
        assert_eq!(schema.meta.file, "server.properties");
        assert_eq!(schema.meta.format, "properties");
        assert_eq!(schema.categories.len(), 2);
        assert_eq!(schema.properties.len(), 3);

        let port_prop = schema.get_property("server-port").expect("port prop");
        assert_eq!(port_prop.property_type, PropertyType::Integer);
        assert_eq!(port_prop.min, Some(1));
        assert_eq!(port_prop.max, Some(65535));

        let cat_props = schema.properties_for_category("general");
        assert_eq!(cat_props.len(), 2);

        // Test GenericProperties
        let raw = "# Comment\nserver-port=25565\nonline-mode=true\ndifficulty=normal\n";
        let mut props = GenericProperties::parse_properties(raw);
        assert_eq!(props.get("server-port"), Some("25565"));
        assert_eq!(props.get("online-mode"), Some("true"));
        assert_eq!(props.get("difficulty"), Some("normal"));

        props.set("server-port", "25570");
        assert_eq!(props.get("server-port"), Some("25570"));
        let dumped = props.dump_properties();
        assert!(dumped.contains("server-port=25570"));
        assert!(dumped.contains("# Comment"));
    }

    #[test]
    fn test_craft_package_roundtrip() {
        let dir = tempdir().expect("tempdir");
        let pkg_src = dir.path().join("my_software");
        std::fs::create_dir_all(&pkg_src).expect("create dir");

        let toml_content = r#"
[software]
id = "my-soft"
name = "My Software"
game = "minecraft"
edition = "java"
"#;
        std::fs::write(pkg_src.join("software.toml"), toml_content).expect("write software.toml");

        let props_content = r#"
[meta]
file = "server.properties"
format = "properties"

[[categories]]
id = "main"
name = "Main"
"#;
        std::fs::write(pkg_src.join("properties.toml"), props_content)
            .expect("write properties.toml");

        let scripts_dir = pkg_src.join("scripts");
        std::fs::create_dir_all(&scripts_dir).expect("create scripts");
        std::fs::write(scripts_dir.join("assets.lua"), "-- assets resolver")
            .expect("write assets.lua");

        // 1. Load from directory
        let bundle_dir = load_from_directory(&pkg_src).expect("load from directory");
        assert_eq!(bundle_dir.id(), "my-soft");
        assert_eq!(bundle_dir.name(), "My Software");
        assert!(bundle_dir.properties_schema.is_some());
        assert_eq!(
            bundle_dir.get_script("scripts/assets.lua"),
            Some("-- assets resolver")
        );

        // 2. Package into .zip file
        let zip_file = dir.path().join("my-soft.zip");
        package_directory(&pkg_src, &zip_file).expect("package directory");
        assert!(zip_file.exists());

        // 3. Load from .zip file
        let bundle_file = load_from_zip_file(&zip_file).expect("load from zip file");
        assert_eq!(bundle_file.id(), "my-soft");
        assert_eq!(bundle_file.name(), "My Software");
        assert!(bundle_file.is_bundle_file);
        assert!(bundle_file.properties_schema.is_some());
        assert_eq!(
            bundle_file.get_script("scripts/assets.lua"),
            Some("-- assets resolver")
        );

        // 4. Test auto-detect load_bundle
        let bundle_auto = load_bundle(&zip_file).expect("auto load bundle");
        assert_eq!(bundle_auto.id(), "my-soft");

        // 5. Test extract_bundle_to_dir
        let extracted_dir = dir.path().join("extracted");
        extract_bundle_to_dir(&bundle_auto, &extracted_dir).expect("extract bundle to dir");
        assert!(extracted_dir.join("software.toml").exists());
        assert!(extracted_dir.join("properties.toml").exists());
        assert!(extracted_dir.join("scripts/assets.lua").exists());
        let bundle_reloaded = load_from_directory(&extracted_dir).expect("reload extracted");
        assert_eq!(bundle_reloaded.id(), "my-soft");
    }

    #[test]
    fn test_craft_fs_extended() {
        let dir = tempdir().expect("tempdir");
        let src_file = dir.path().join("source.txt");
        let dst_file = dir.path().join("copy.txt");
        let src_path_str = src_file.to_string_lossy().replace('\\', "/");
        let dst_path_str = dst_file.to_string_lossy().replace('\\', "/");

        let engine = LuaEngine::new().expect("engine");
        engine
            .exec(&format!(
                "craft.fs.write('{}', 'extended fs test')",
                src_path_str
            ))
            .expect("write");

        // size
        let size: u64 = engine
            .eval(&format!("return craft.fs.size('{}')", src_path_str))
            .expect("size");
        assert_eq!(size, 16);

        // copy
        let copied: bool = engine
            .eval(&format!(
                "return craft.fs.copy('{}', '{}')",
                src_path_str, dst_path_str
            ))
            .expect("copy");
        assert!(copied);
        assert!(dst_file.exists());

        // remove
        let removed: bool = engine
            .eval(&format!("return craft.fs.remove('{}')", dst_path_str))
            .expect("remove");
        assert!(removed);
        assert!(!dst_file.exists());
    }

    #[test]
    fn test_craft_servers_and_backup_api() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join(".craft");
        std::fs::create_dir_all(&root).unwrap();
        let paths = craft_core::CraftPaths::from_base(root);

        let engine = LuaEngine::new_with_paths(&paths).expect("engine");
        let servers: mlua::Table = engine
            .eval("return craft.servers.list()")
            .expect("servers list");
        assert_eq!(servers.len().unwrap_or(0), 0);

        let backups: mlua::Table = engine
            .eval("return craft.backup.list('lobby')")
            .expect("backup list");
        assert_eq!(backups.len().unwrap_or(0), 0);
    }

    #[test]
    fn test_craft_audit_api() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join(".craft");
        std::fs::create_dir_all(&root).unwrap();
        let paths = craft_core::CraftPaths::from_base(root);

        let engine = LuaEngine::new_with_paths(&paths).expect("engine");
        let ok: bool = engine
            .eval("return craft.audit.log('server_start', 'admin', 'test audit')")
            .expect("audit log");
        assert!(ok);
        assert!(paths.audit_file.exists());
    }

    #[test]
    fn test_eval_timeout_guard() {
        let engine = LuaEngine::new().expect("engine");
        // Infinite loop should time out
        let res: std::result::Result<i32, _> =
            engine.eval_with_timeout("while true do end return 1", 1);
        assert!(res.is_err());
        let err_msg = res.err().unwrap().to_string();
        assert!(err_msg.contains("timed out"));
    }

    #[test]
    fn test_hook_bus_discovery_and_dispatch() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join(".craft");
        std::fs::create_dir_all(&root).unwrap();
        let paths = craft_core::CraftPaths::from_base(root);

        let hooks_dir = HookBus::ensure_hooks_dir(&paths).expect("hooks dir");
        let hook_file = hooks_dir.join("on_server_crash.lua");
        std::fs::write(
            &hook_file,
            "craft.log('HOOK EXECUTED: crash on ' .. ctx.server_name)",
        )
        .expect("write hook");

        let discovered = HookBus::discover_hooks(&paths, None);
        let crash_hook = discovered
            .iter()
            .find(|h| h.name == "on_server_crash.lua")
            .expect("found");
        assert!(crash_hook.active);

        let mut ctx = HookContext::new(LifecycleEvent::ServerCrash);
        ctx.server_name = Some("lobby".to_string());
        ctx.exit_code = Some(1);

        let results = HookBus::dispatch(&paths, LifecycleEvent::ServerCrash, &ctx, 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert_eq!(results[0].hook_name, "on_server_crash.lua");
    }
}
