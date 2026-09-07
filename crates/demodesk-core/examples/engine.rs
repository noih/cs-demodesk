use demodesk_core::engine::{Engine, Event, Notify};
use demodesk_core::store::DemoStatus;
use std::path::{Path, PathBuf};
use std::sync::Arc;
struct Print;
impl Notify for Print {
    fn notify(&self, e: Event) {
        if let Event::DemoChanged { demo } = e {
            println!("event: {} {:?} {:?}", demo.name, demo.status, demo.summary.as_ref().map(|s| (s.score_a, s.score_b, s.highlights)));
        }
    }
}
fn main() {
    let demo = PathBuf::from(std::env::args().nth(1).expect("demo"));
    let data = std::env::temp_dir().join("demodesk-engine-test");
    let _ = std::fs::remove_dir_all(&data);
    let id;
    {
        let engine = Engine::new(data.clone(), Arc::new(Print)).unwrap();
        let mut settings = engine.settings();
        settings.replay_folders = vec![demo.parent().unwrap().to_string_lossy().to_string()];
        engine.save_settings(settings).unwrap();
        let meta = engine.add_demo(Path::new(&demo)).unwrap();
        id = meta.id.clone();
        println!("added {} {} status {:?}", meta.id, meta.name, meta.status);
        let t = std::time::Instant::now();
        engine.parse_demo(&meta.id).unwrap();
        while engine.is_parsing(&meta.id) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        println!("parsed in {:.1}s: {} highlights", t.elapsed().as_secs_f64(), engine.parsed(&meta.id).unwrap().highlights.len());
    }
    let list = |p: &Path| std::fs::read_dir(p).unwrap().flatten().map(|e| format!("{} ({} B)", e.file_name().to_string_lossy(), e.metadata().map(|m| m.len()).unwrap_or(0))).collect::<Vec<_>>();
    println!("parsed dir: {:?}", list(&data.join("parsed")));

    // second "session": must come back parsed without parsing
    let engine = Engine::new(data.clone(), Arc::new(Print)).unwrap();
    let t = std::time::Instant::now();
    let demos = engine.list_demos();
    let m = demos.iter().find(|m| m.id == id).unwrap();
    println!("after restart: status {:?} summary {:?} (list took {:.0} ms)", m.status, m.summary.as_ref().map(|s| (s.score_a, s.score_b, s.highlights)), t.elapsed().as_millis());
    assert_eq!(m.status, DemoStatus::Parsed);
    let t = std::time::Instant::now();
    let parsed = engine.parsed(&id).unwrap();
    println!("lazy load: {} highlights in {:.0} ms", parsed.highlights.len(), t.elapsed().as_millis());

    engine.clear_analysis(&id).unwrap();
    println!("after clear: {:?}, parsed dir {:?}", engine.get_demo(&id).unwrap().0.status, list(&data.join("parsed")));
    println!("clear_all freed {} B", engine.clear_all_analysis().unwrap());
}
