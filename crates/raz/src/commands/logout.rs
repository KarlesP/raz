//! `raz logout` — clears the persisted profile. Mirrors az `profile/custom.py::logout`.

use raz_core::config::Profile;
use raz_core::error::Result;

pub async fn run() -> Result<()> {
    Profile::clear()?;
    println!("Logged out. Cleared ~/.raz profile.");
    Ok(())
}
