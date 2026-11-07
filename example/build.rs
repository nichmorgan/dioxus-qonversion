const KEYS: [&str; 3] = [
    "QONVERSION_PROJECT_KEY",
    "QONVERSION_REMOTE_CONFIG_CONTEXT_KEY",
    "QONVERSION_NO_CODE_CONTEXT_KEY",
];

fn main() {
    println!("cargo:rerun-if-changed=.env");
    let file: Vec<(String, String)> = dotenvy::from_path_iter(".env")
        .map(|iter| iter.flatten().collect())
        .unwrap_or_default();
    for key in KEYS {
        println!("cargo:rerun-if-env-changed={key}");
        let non_empty = |value: &String| !value.trim().is_empty();
        let value = std::env::var(key)
            .ok()
            .filter(non_empty)
            .or_else(|| {
                file.iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, value)| value.clone())
            })
            .filter(non_empty);
        if let Some(value) = value {
            println!("cargo:rustc-env={key}={value}");
        }
    }
}
