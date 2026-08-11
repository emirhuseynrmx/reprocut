#[cfg(test)]
mod manifest_contract {
    use crate::reprocut_adapters::{
        CargoManifest, ManifestCapability, NpmManifest, PythonManifest,
    };

    #[test]
    fn cargo_python_and_npm_edits_are_capability_aware() {
        let mut cargo = CargoManifest::parse(
            "[package]\nname='x'\nversion='0.1.0'\n[dependencies]\nserde='1'\nregex='1'\n",
        )
        .expect("cargo");
        let regex = cargo
            .entries()
            .into_iter()
            .find(|entry| entry.stable_key() == "cargo:dependencies.regex")
            .expect("regex");
        cargo.remove(&regex).expect("remove cargo dependency");
        assert!(!cargo.render().contains("regex='1'"));
        assert!(!CargoManifest::preparation().network_allowed());

        let mut python =
            PythonManifest::parse("[project]\nname='x'\ndependencies=['requests', 'numpy']\n")
                .expect("python");
        let requests = python
            .entries()
            .into_iter()
            .find(|entry| entry.stable_key().contains("requests"))
            .expect("requests");
        assert_eq!(
            requests.capability(),
            ManifestCapability::RequiresIsolatedPython
        );
        python.remove(&requests).expect("remove Python dependency");
        assert!(!python.render().contains("requests"));

        let mut npm = NpmManifest::parse(
            r#"{"scripts":{"test":"jest","prepare":"node build.js"},"dependencies":{"x":"1"}}"#,
        )
        .expect("npm");
        let dependency = npm
            .entries()
            .into_iter()
            .find(|entry| entry.stable_key() == "npm:dependencies.x")
            .expect("npm dependency");
        npm.remove(&dependency).expect("remove npm dependency");
        assert!(!npm.render().expect("render").contains("\"x\""));
        let plan = NpmManifest::preparation(false);
        assert!(!plan.network_allowed());
        assert!(!plan.lifecycle_scripts_allowed());
    }
}
