pub mod config;
pub mod engine;
pub mod starter;

pub use config::{CustomRuntimeType, CustomServerConfig, CUSTOM_CONFIG_FILE};
pub use engine::LuaEngine;
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
}
