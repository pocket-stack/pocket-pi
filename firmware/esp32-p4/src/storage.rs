use esp_idf_svc::fs::littlefs::Littlefs;
use esp_idf_svc::io::vfs::MountedLittlefs;

pub const WORKSPACE_ROOT: &str = "/workspace";
const PARTITION_LABEL: &str = "workspace";

pub type WorkspaceMount = MountedLittlefs<Littlefs<()>>;

pub fn mount_workspace() -> anyhow::Result<WorkspaceMount> {
    let fs = unsafe { Littlefs::<()>::new_partition(PARTITION_LABEL)? };
    match MountedLittlefs::mount(fs, WORKSPACE_ROOT) {
        Ok(mounted) => Ok(mounted),
        Err(_mount_error) if partition_is_blank()? => {
            let mut fs = unsafe { Littlefs::<()>::new_partition(PARTITION_LABEL)? };
            fs.format()?;
            MountedLittlefs::mount(fs, WORKSPACE_ROOT).map_err(Into::into)
        }
        Err(mount_error) => Err(anyhow::anyhow!(
            "LittleFS workspace mount failed; preserving non-blank partition: {mount_error}"
        )),
    }
}

pub fn workspace_free_bytes() -> anyhow::Result<u64> {
    let mut total = 0usize;
    let mut used = 0usize;
    let status = unsafe {
        esp_idf_svc::sys::esp_littlefs_info(c"workspace".as_ptr(), &mut total, &mut used)
    };
    if status != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("read LittleFS workspace capacity: ESP error {status}")
    }
    Ok(total.saturating_sub(used) as u64)
}

fn partition_is_blank() -> anyhow::Result<bool> {
    let partition = unsafe {
        esp_idf_svc::sys::esp_partition_find_first(
            esp_idf_svc::sys::esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
            esp_idf_svc::sys::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_DATA_LITTLEFS,
            c"workspace".as_ptr(),
        )
    };
    if partition.is_null() {
        anyhow::bail!("LittleFS workspace partition is missing");
    }
    let mut prefix = [0u8; 4096];
    let status = unsafe {
        esp_idf_svc::sys::esp_partition_read(partition, 0, prefix.as_mut_ptr().cast(), prefix.len())
    };
    if status != esp_idf_svc::sys::ESP_OK {
        anyhow::bail!("read LittleFS workspace partition: ESP error {status}");
    }
    Ok(prefix.iter().all(|byte| *byte == 0xff))
}
