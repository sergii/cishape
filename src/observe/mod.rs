use crate::model::RunObservation;
use anyhow::Result;

#[cfg(target_os = "linux")]
mod linux;

pub fn command(job: &str, program: &str, args: &[String]) -> Result<RunObservation> {
    #[cfg(target_os = "linux")]
    {
        return linux::command(job, program, args);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (job, program, args);
        anyhow::bail!("cishape observe is Linux-only in OBSERVE1");
    }
}
