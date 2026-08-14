use super::*;

/// Install a registered native engine. Unknown and retired engine IDs are
/// rejected before any download or subprocess can begin.
pub async fn install_engine(app: &crate::emit::EventBus, engine_id: &str) -> Result<(), String> {
    if engine_id != "paddleocr-vl-parser" {
        return Err(format!("Unknown engine '{engine_id}'"));
    }
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    reset_install_progress(engine_id);
    INSTALL_CANCEL.store(false, Ordering::Release);
    let removed = cleanup_managed_engine_storage_locked()?;
    if removed > 0 {
        log(
            app,
            format!("Removed {removed} obsolete managed-engine item(s)"),
        );
    }

    // A sidecar-only parser update reuses the already verified private
    // runtime and does not need the full installation's disk headroom.
    let lightweight_parser_refresh = engine_id == "paddleocr-vl-parser"
        && paddle_parser_root()
            .ok()
            .is_some_and(|root| parser_runtime_reusable_for_sidecar_refresh(&root));

    // Check only the components this action must add. Existing targets already
    // consume their disk space; rollback-safe replacement needs one new copy
    // plus bounded archive/cache headroom, not the whole shared stack again.
    let home = pipeline_home()?;
    std::fs::create_dir_all(&home).map_err(|e| format!("Failed to create ~/.pipeline: {e}"))?;
    let base_installed = paddle_root()
        .ok()
        .is_some_and(|root| paddle_paths_at(&root).is_some());
    let parser_target_present = paddle_parser_version_root()
        .ok()
        .is_some_and(|target| target.exists());
    let (space_plan, required_mb) = install_space_requirement(
        engine_id,
        base_installed,
        parser_target_present,
        lightweight_parser_refresh,
    )?;
    if let Ok(free) = fs2::available_space(&home) {
        if let Err(error) = check_install_space(free, space_plan, required_mb) {
            emit_phase(app, engine_id, "runtime", "failed");
            return Err(format!("{} ({})", error, spec.label));
        }
    }

    let result = install_paddle_full_parser(app, spec).await;
    if result.is_ok() {
        match cleanup_managed_engine_storage_locked() {
            Ok(removed) if removed > 0 => log(
                app,
                format!("Removed {removed} temporary managed-engine item(s)"),
            ),
            Ok(_) => {}
            Err(error) => log(
                app,
                format!("Warning: managed-engine cleanup will retry at next startup: {error}"),
            ),
        }
    }
    result
}

/// Uninstall a registered native engine. Retired engine IDs are intentionally
/// rejected; Pipeline never executes their package managers or entry points.
pub async fn uninstall_engine(app: &crate::emit::EventBus, engine_id: &str) -> Result<(), String> {
    if engine_id != "paddleocr-vl-parser" {
        return Err(format!("Unknown engine '{engine_id}'"));
    }
    let spec = engine(engine_id)?;
    let _guard = acquire_install_guard()?;
    INSTALL_CANCEL.store(false, Ordering::Release);
    cleanup_managed_engine_storage_locked()?;

    let roots = [paddle_parser_root()?, paddle_root()?];
    tokio::task::spawn_blocking(move || {
        for root in roots {
            remove_owned_path(&root)?;
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("Managed engine cleanup task failed: {error}"))??;
    log(app, format!("{} uninstalled", spec.label));
    Ok(())
}
