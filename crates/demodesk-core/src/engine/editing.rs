use super::*;
use crate::model::Highlight;
use crate::render::editing::{Analysis, Plan, Summary, VERSION};
use anyhow::ensure;

fn selected(parsed: &ParsedDemo, ids: &[String]) -> Result<Vec<Highlight>> {
    let wanted: HashSet<_> = ids.iter().collect();
    let mut clips: Vec<_> = parsed
        .highlights
        .iter()
        .filter(|h| wanted.contains(&h.id))
        .cloned()
        .collect();
    ensure!(
        !clips.is_empty() && clips.len() == ids.len() && wanted.len() == ids.len(),
        "Invalid highlight selection"
    );
    clips.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(clips)
}

fn key(
    meta: &DemoMeta,
    parsed: &ParsedDemo,
    clips: &[Highlight],
    options: &RenderOptions,
) -> Result<String> {
    let file = std::fs::metadata(&meta.path)?;
    let modified = file
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let source = serde_json::to_vec(&(
        VERSION,
        &meta.path,
        file.len(),
        modified.to_string(),
        &parsed.parsed_at,
        clips,
        options.key_moments_only,
        options.short_combat_clips,
    ))?;
    Ok(sha1_smol::Sha1::from(source).digest().to_string())
}

impl Engine {
    pub fn preview_editing_plan(
        &self,
        demo_id: &str,
        ids: &[String],
        options: &RenderOptions,
    ) -> Result<Vec<crate::render::editing::PreviewShot>> {
        let (meta, parsed) = self
            .get_demo(demo_id)
            .ok_or(crate::ErrorCode::DemoMissing)?;
        let parsed = parsed.ok_or(crate::ErrorCode::DemoNotReady)?;
        ensure!(
            meta.status == DemoStatus::Parsed,
            crate::ErrorCode::DemoNotReady
        );
        let originals = selected(&parsed, ids)?;
        let plan = self
            .selected_editing_plan(&meta, &parsed, ids, options)?
            .ok_or_else(|| anyhow::anyhow!("Editing preview requires analysis"))?;
        Ok(plan.preview(&originals))
    }

    fn editing_path(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid editing analysis ID"
        );
        Ok(self
            .data_dir
            .join("analysis/editing")
            .join(format!("{id}.json")))
    }

    pub fn cached_editing_analysis(
        &self,
        demo_id: &str,
        ids: &[String],
        options: &RenderOptions,
    ) -> Result<Option<Summary>> {
        let _guard = self.editing_lock.lock().unwrap();
        let (meta, parsed) = self
            .get_demo(demo_id)
            .ok_or(crate::ErrorCode::DemoMissing)?;
        let parsed = parsed.ok_or(crate::ErrorCode::DemoNotReady)?;
        ensure!(
            meta.status == DemoStatus::Parsed,
            crate::ErrorCode::DemoNotReady
        );
        let clips = selected(&parsed, ids)?;
        let id = key(&meta, &parsed, &clips, options)?;
        Ok(cached_summary(&self.editing_path(&id)?, &id))
    }

    pub fn analyze_editing(
        &self,
        demo_id: &str,
        ids: &[String],
        options: &RenderOptions,
        force: bool,
    ) -> Result<Summary> {
        let _guard = self.editing_lock.lock().unwrap();
        let (meta, parsed) = self
            .get_demo(demo_id)
            .ok_or(crate::ErrorCode::DemoMissing)?;
        let parsed = parsed.ok_or(crate::ErrorCode::DemoNotReady)?;
        ensure!(
            meta.status == DemoStatus::Parsed,
            crate::ErrorCode::DemoNotReady
        );
        let clips = selected(&parsed, ids)?;
        let id = key(&meta, &parsed, &clips, options)?;
        let path = self.editing_path(&id)?;
        if force {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        } else if let Some(summary) = cached_summary(&path, &id) {
            return Ok(summary);
        }
        let mut demo = parsed.info.clone();
        demo.path = meta.path.clone();
        let _tools = self.tool_access.use_tools(&[SetupTool::Vrf]);
        let analysis = Analysis::prepare(
            id.clone(),
            clips.clone(),
            &demo,
            options,
            &self.tool_paths(),
            &self.data_dir,
        )?;
        let _data = self.data_lock.lock().unwrap();
        ensure!(
            self.get_demo(demo_id)
                .is_some_and(|(current, result)| current.status == DemoStatus::Parsed
                    && result.is_some_and(|p| Arc::ptr_eq(&p, &parsed))),
            "Demo changed during analysis; try again"
        );
        ensure!(
            id == key(&meta, &parsed, &clips, options)?,
            "Demo changed during analysis; try again"
        );
        std::fs::create_dir_all(path.parent().unwrap())?;
        crate::store::write_atomic(&path, &serde_json::to_vec(&analysis)?)?;
        Ok(analysis.summary(false))
    }

    pub fn clear_editing_analysis(&self, id: &str) -> Result<()> {
        let _guard = self.editing_lock.lock().unwrap();
        match std::fs::remove_file(self.editing_path(id)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    pub(super) fn selected_editing_plan(
        &self,
        meta: &DemoMeta,
        parsed: &ParsedDemo,
        ids: &[String],
        options: &RenderOptions,
    ) -> Result<Option<Plan>> {
        let Some(id) = &options.editing_analysis_id else {
            ensure!(
                options.editing_selections.is_empty() && options.editing_replay_links.is_empty(),
                "Editing selections require analysis"
            );
            return Ok(None);
        };
        let _guard = self.editing_lock.lock().unwrap();
        let clips = selected(parsed, ids)?;
        ensure!(
            *id == key(meta, parsed, &clips, options)?,
            "Editing analysis is out of date; analyze again"
        );
        let analysis: Analysis = serde_json::from_slice(&std::fs::read(self.editing_path(id)?)?)?;
        ensure!(analysis.id == *id, "Invalid editing analysis");
        Ok(Some(analysis.select_with_links(
            &options.editing_selections,
            &options.editing_replay_links,
            &parsed.info,
        )?))
    }

    pub(super) fn saved_editing_plan(
        &self,
        job: &RenderJob,
        meta: &DemoMeta,
        parsed: &ParsedDemo,
    ) -> Result<Option<Plan>> {
        let Some(id) = &job.options.editing_analysis_id else {
            return Ok(None);
        };
        let clips = selected(parsed, &job.highlight_ids)?;
        ensure!(
            *id == key(meta, parsed, &clips, &job.options)?,
            "Demo changed since editing analysis; analyze again"
        );
        // A queued job owns its plan; clearing the analysis cache cannot change it.
        Ok(Some(serde_json::from_slice(&std::fs::read(
            self.store.job_dir(&job.id).join("editing-plan.json"),
        )?)?))
    }
}

fn cached_summary(path: &Path, id: &str) -> Option<Summary> {
    let analysis: Analysis = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    (analysis.version == VERSION && analysis.id == id).then(|| analysis.summary(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Silent;
    impl Notify for Silent {
        fn notify(&self, _: Event) {}
    }

    #[test]
    fn cached_analysis_is_offline_and_jobs_keep_their_snapshot() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = root.path().join("test.dem");
        // An invalid demo proves the cache hit and plan selection do not parse it.
        std::fs::write(&source, b"not a valid demo header or body")?;
        let engine = Engine::new(root.path().join("data"), Arc::new(Silent))?;
        engine.render_worker_running.store(true, Ordering::SeqCst);
        let h = json!({"id":"one","player":{"steamid":"main","name":"Main"},"round":1,"startTick":800,"endTick":2000,"anchorTick":1000,"score":1,"tags":[],"title":"clip","kills":[],"breakdown":{}});
        let parsed: ParsedDemo = serde_json::from_value(
            json!({"info":{"path":source,"mapName":"de_test","serverName":"test","tickRate":64,"players":[{"steamid":"main","name":"Main","teamNumber":3,"userId":0}]},"rounds":[],"kills":[],"highlights":[h],"stats":[],"score":{},"roundSummaries":[],"parsedAt":"v1"}),
        )?;
        let meta: DemoMeta = serde_json::from_value(
            json!({"id":"demo","name":"test.dem","path":source,"bytes":10,"mtimeMs":0,"createdMs":0,"status":"parsed"}),
        )?;
        engine.demos.lock().unwrap().insert(
            "demo".into(),
            DemoEntry {
                meta: meta.clone(),
                parsed: Some(Arc::new(parsed.clone())),
                auto_complete: None,
            },
        );
        let mut options = RenderOptions::default();
        let ids = vec!["one".into()];
        let id = key(&meta, &parsed, &parsed.highlights, &options)?;
        let path = engine.editing_path(&id)?;
        assert!(engine
            .cached_editing_analysis("demo", &ids, &options)?
            .is_none());
        let analysis = json!({"version":VERSION,"id":id,"candidates":[],"warnings":[],"originals":[h],"recording":[h],"features":[]});
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_vec(&analysis)?)?;
        assert!(
            engine
                .cached_editing_analysis("demo", &ids, &options)?
                .unwrap()
                .cached
        );
        assert!(
            engine
                .analyze_editing("demo", &ids, &options, false)?
                .cached
        );
        options.editing_analysis_id = Some(id.clone());
        options.editing_selections = vec!["invented".into()];
        assert!(engine
            .enqueue_render("demo", ids.clone(), options.clone())
            .is_err());
        assert!(engine.store.list_jobs().is_empty());
        options.editing_selections.clear();
        let preview = engine.preview_editing_plan("demo", &ids, &options)?;
        assert_eq!(preview.len(), 1);
        assert_eq!(
            (preview[0].kind, preview[0].start_tick, preview[0].end_tick),
            ("main", 800, 2000)
        );
        options
            .editing_replay_links
            .push(crate::render::editing::ReplayLink {
                from: "unknown".into(),
                to: "other".into(),
            });
        assert!(engine.preview_editing_plan("demo", &ids, &options).is_err());
        options.editing_replay_links.clear();
        let job = engine.enqueue_render("demo", ids.clone(), options.clone())?;
        engine.clear_editing_analysis(&id)?;
        assert!(!path.exists());
        assert!(engine.preview_editing_plan("demo", &ids, &options).is_err());
        assert!(engine.saved_editing_plan(&job, &meta, &parsed)?.is_some());
        assert!(engine
            .selected_editing_plan(&meta, &parsed, &ids, &options)
            .is_err());
        assert!(engine.clear_editing_analysis("../../parsed/test").is_err());
        let mut changed = options.clone();
        changed.short_combat_clips = true;
        assert_ne!(id, key(&meta, &parsed, &parsed.highlights, &changed)?);
        changed = options.clone();
        changed.width = 1280;
        assert_eq!(id, key(&meta, &parsed, &parsed.highlights, &changed)?);
        // Forced analysis clears the old result even when recomputation fails.
        std::fs::write(&path, serde_json::to_vec(&analysis)?)?;
        assert!(engine
            .analyze_editing("demo", &ids, &options, true)
            .is_err());
        assert!(!path.exists());
        assert!(engine
            .cached_editing_analysis("demo", &ids, &options)?
            .is_none());
        std::fs::write(&source, b"changed source")?;
        assert!(engine.saved_editing_plan(&job, &meta, &parsed).is_err());
        assert!(selected(&parsed, &["one".into(), "one".into()]).is_err());
        Ok(())
    }
}
