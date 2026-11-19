//! Commit libre real de Windows: `(CommitLimit − CommitTotal) × PageSize`.

#[cfg(windows)]
pub fn free_and_total_mb() -> Option<(u64, u64)> {
    use windows::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    let cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let mut info = PERFORMANCE_INFORMATION {
        cb,
        ..Default::default()
    };
    // SAFETY: `info` es una estructura local válida y `cb` declara su tamaño;
    // GetPerformanceInfo solo escribe en ella durante la llamada y no la retiene.
    unsafe { GetPerformanceInfo(&mut info, cb) }.ok()?;
    let page = info.PageSize as u64;
    let free = info.CommitLimit.saturating_sub(info.CommitTotal) as u64;
    let total = info.CommitLimit as u64;
    Some((
        free.saturating_mul(page) / 1_048_576,
        total.saturating_mul(page) / 1_048_576,
    ))
}

#[cfg(not(windows))]
pub fn free_and_total_mb() -> Option<(u64, u64)> {
    None
}
