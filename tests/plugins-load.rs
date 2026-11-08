use std::{collections::HashSet, path::PathBuf};

use sleepy_doll::extension::{
    ToolEffect, ToolExposure, ToolRegistry,
    plugins::{PluginManager, PluginManifest, PluginState},
    skills::SkillRegistry,
};

const IDS: [&str; 3] = ["march7th", "maaend", "zzz"];

fn plugin_root() -> PathBuf {
    std::env::var_os("SLEEPY_DOLL_PLUGIN_FIXTURE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins"))
}

#[test]
fn independent_bridges_load_tools_and_skills_without_starting_hosts() {
    let root = plugin_root();
    let mut endpoints = HashSet::new();
    for id in IDS {
        let manifest: PluginManifest = serde_json::from_slice(
            &std::fs::read(root.join(id).join(".sleepy-doll-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest.id, id);
        assert!(manifest.mcp_servers.is_empty(), "不应启动上游 MCP");
        assert!(manifest.adapters.is_empty(), "装载插件不应启动子进程");
        assert!(!manifest.http_tools.is_empty());
        let mut plugin_endpoints = HashSet::new();
        for tool in &manifest.http_tools {
            tool.execution.validate().unwrap();
            assert!(tool.execution.deferred);
            assert!(!tool.execution.always_load && !tool.execution.prompt_visible);
            assert_eq!(tool.execution.exposure, ToolExposure::OnDemand);
            if tool.name.ends_with(".search") {
                assert!(tool.execution.search_provider, "目录检索应可被通用检索发现");
            }
            jsonschema::validator_for(&tool.input_schema).unwrap();
            if let Some(schema) = &tool.output_schema {
                jsonschema::validator_for(schema).unwrap();
            }
            let url = url::Url::parse(&tool.url).unwrap();
            assert_eq!(url.scheme(), "http");
            assert_eq!(url.host_str(), Some("127.0.0.1"));
            plugin_endpoints.insert(url.port().unwrap());
            assert!(
                tool.headers
                    .values()
                    .any(|value| value.starts_with("${ENV:"))
            );
            assert_ne!(tool.execution.effect, ToolEffect::Unknown);
        }
        assert_eq!(plugin_endpoints.len(), 1, "一个插件只连接自己的桥");
        assert!(endpoints.insert(*plugin_endpoints.iter().next().unwrap()));
    }

    let mut manager = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    manager
        .load(
            &[root],
            &IDS.map(str::to_owned),
            &[
                "bgi".into(),
                "windows-scheduler".into(),
                "web-search".into(),
            ],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    for id in IDS {
        let status = manager.list().iter().find(|s| s.manifest.id == id).unwrap();
        assert_eq!(
            status.state,
            PluginState::Enabled,
            "{id}: {:?}",
            status.error
        );
        assert!(
            tools
                .definitions()
                .iter()
                .any(|t| t.source == format!("plugin:{id}:http"))
        );
        assert!(skills.list().iter().any(|s| s.source.plugin() == Some(id)));
        for skill in skills
            .list()
            .iter()
            .filter(|skill| skill.source.plugin() == Some(id))
        {
            assert!(
                skill.body.chars().count() < 1200,
                "入口只保留领域约定与引用"
            );
            for path in [
                "references/tasks.md",
                "references/config.md",
                "references/connection.md",
            ] {
                assert!(skill.body.contains(path));
                let reference = skills.read_reference(&skill.name, path).unwrap();
                assert!(!reference.is_empty());
                for forbidden in [
                    ".http.",
                    "currencywars",
                    "货币战争",
                    "catalogHash",
                    "instanceId",
                    "taskId",
                ] {
                    assert!(
                        !reference.contains(forbidden),
                        "具体调用和对象只从当前契约取得：{id}/{path}"
                    );
                    assert!(!skill.body.contains(forbidden));
                    assert!(!skill.description.contains(forbidden));
                }
            }
        }
    }
    assert!(manager.adapters().is_empty());
}

#[test]
fn independent_bridges_are_disabled_by_default() {
    let mut manager = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    manager
        .load(
            &[plugin_root()],
            &[],
            &[
                "bgi".into(),
                "windows-scheduler".into(),
                "web-search".into(),
            ],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    for id in IDS {
        assert_eq!(
            manager
                .list()
                .iter()
                .find(|s| s.manifest.id == id)
                .unwrap()
                .state,
            PluginState::Disabled,
        );
    }
    assert!(tools.is_empty());
    assert!(skills.is_empty());
}

#[test]
#[ignore = "需要已组装的独立插件包，只导入隔离用户目录，不启动桥"]
fn packaged_plugins_import_without_starting_hosts() {
    let source = PathBuf::from(
        std::env::var_os("SLEEPY_DOLL_PLUGIN_FIXTURE_ROOT")
            .expect("必须明确指定已经校验的独立插件交付目录"),
    )
    .canonicalize()
    .unwrap();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert_eq!(
        source,
        workspace.join("dist/plugins").canonicalize().unwrap()
    );
    let config_dir = workspace.join("target/.tmp/local-adapters/plugin-import");
    assert!(
        !config_dir.exists(),
        "隔离导入目录必须为空，已有内容需核实清理"
    );
    std::fs::create_dir_all(&config_dir).unwrap();
    let mut config: sleepy_doll::AppConfig =
        serde_json::from_str(include_str!("../sleepy-doll.config.example.json")).unwrap();
    config.plugins.directories = vec![config_dir.join("plugins")];
    config.plugins.enabled.clear();
    for id in IDS {
        let result = sleepy_doll::runtime::host::installation::install(
            &config_dir,
            &config,
            &source.join(id),
        )
        .unwrap();
        assert_eq!(result, id);
        let installed = config_dir.join("plugins").join(id);
        for relative in [
            ".sleepy-doll-plugin/plugin.json",
            "bridge-package.json",
            "start-bridge.ps1",
        ] {
            assert_eq!(
                std::fs::read(source.join(id).join(relative)).unwrap(),
                std::fs::read(installed.join(relative)).unwrap(),
                "导入应原样保存文件：{id}/{relative}",
            );
        }
    }
    let mut manager = PluginManager::default();
    let mut tools = ToolRegistry::default();
    let mut skills = SkillRegistry::default();
    manager
        .load(
            &config.plugins.directories,
            &[],
            &[],
            &mut tools,
            &mut skills,
        )
        .unwrap();
    assert_eq!(manager.list().len(), 3);
    assert!(
        manager
            .list()
            .iter()
            .all(|item| item.state == PluginState::Disabled)
    );
    assert!(tools.is_empty());
    assert!(skills.is_empty());
    assert!(manager.adapters().is_empty());
}
