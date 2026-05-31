use std::sync::OnceLock;

pub(crate) fn ensure_keyring_store() -> Result<(), String> {
    static KEYRING_INIT: OnceLock<Result<(), String>> = OnceLock::new();
    KEYRING_INIT
        .get_or_init(|| {
            keyring::use_native_store(true)
                .map_err(|error| format!("初始化系统 Keychain 失败：{error}"))
        })
        .clone()
}
