//! Installed workflow profile listing and inspection.

use super::args::ProfilesArgs;
use super::io::emit_stdout;
use pipeline_gui_lib::pipeline_config;

pub(super) fn cmd_profiles(args: ProfilesArgs) -> i32 {
    match args {
        ProfilesArgs::Show { id } => match pipeline_config::export_profile_data(&id) {
            Ok(profile) => {
                emit_stdout(&profile);
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
        ProfilesArgs::List { json } => match pipeline_config::list_profiles() {
            Ok(profiles) if profiles.is_empty() => {
                eprintln!(
                    "Error: no profiles were found; Pipeline may not have been able to initialize ~/.pipeline/profiles"
                );
                1
            }
            Ok(profiles) if json => match serde_json::to_string_pretty(&profiles) {
                Ok(encoded) => {
                    emit_stdout(&encoded);
                    0
                }
                Err(error) => {
                    eprintln!("Error: failed to encode profiles: {error}");
                    1
                }
            },
            Ok(profiles) => {
                let active = pipeline_config::get_active_profile_id();
                for profile in profiles {
                    let marker = if profile.id == active { "*" } else { " " };
                    emit_stdout(&format!(
                        "{marker} {:22} {} ({} steps)",
                        profile.id, profile.name, profile.step_count
                    ));
                }
                0
            }
            Err(error) => {
                eprintln!("Error: {error}");
                1
            }
        },
    }
}
