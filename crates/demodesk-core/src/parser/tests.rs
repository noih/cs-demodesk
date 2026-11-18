use super::*;

struct FixtureBackend;
impl DemoQueries for FixtureBackend {
    fn header(&self, _: &[u8]) -> Result<HashMap<String, String>> {
        Ok(HashMap::from([("map_name".into(), "fixture".into())]))
    }
    fn server_info(&self, _: &[u8]) -> Result<Vec<ServerInfo>> {
        Ok(vec![ServerInfo {
            tick_interval: Some(1. / 128.),
            ..Default::default()
        }])
    }
    fn property_names(&self, _: &[u8]) -> Result<Vec<String>> {
        Ok(vec!["X".into()])
    }
    fn events(&self, _: &[u8], _: &[String], _: &[String], _: &[String]) -> Result<EventBatch> {
        Ok(EventBatch {
            game_events: vec![],
            server_infos: vec![],
            players: vec![],
        })
    }
    fn ticks(&self, _: &[u8], _: &[String], _: Vec<i32>) -> Result<Rows> {
        Ok(Rows::from_columns(
            vec![
                ("tick".into(), Column::I32(vec![Some(2), Some(1)])),
                ("steamid".into(), Column::U64(vec![Some(u64::MAX), None])),
                (
                    "position".into(),
                    Column::XYZVec(vec![Some([1.25, -2.5, 3.75]), None]),
                ),
            ],
            2,
        ))
    }
    fn projectiles(&self, _: &[u8], _: Vec<i32>) -> Result<Rows> {
        Ok(Rows::from_columns(vec![], 0))
    }
}
impl DemoStream for FixtureBackend {
    fn visit_scene_frames(
        &self,
        _: &[u8],
        _: i32,
        _: i32,
        _: i32,
        _: bool,
        visit: &mut dyn FnMut(SceneFrame) -> Result<()>,
    ) -> Result<()> {
        visit(SceneFrame {
            tick: 7,
            net_tick: 7,
            entities: vec![],
        })
    }
    fn write_match_state(&self, _: &[u8], output: &mut dyn Write) -> Result<()> {
        output.write_all(b"fixture")?;
        Ok(())
    }
    fn smoke_replay(
        &self,
        _: &[u8],
        _: i32,
        _: i32,
        _: f64,
        _: &[ReplayEvent],
    ) -> Result<Vec<Snapshot>> {
        Ok(vec![])
    }
}
#[test]
fn configured_backend_owns_queries_and_streaming_without_vendor_types() {
    let parser = DemoParser::with_backend(FixtureBackend);
    assert_eq!(parser.header(&[]).unwrap()["map_name"], "fixture");
    let rows = parser.ticks(&[], &[], vec![]).unwrap();
    let rows: Vec<_> = rows.iter().collect();
    assert_eq!(rows[0].tick(), Some(1));
    assert_eq!(rows[0].steamid(), None);
    assert_eq!(rows[0].vec3("position"), None);
    assert_eq!(rows[1].steamid(), Some(u64::MAX.to_string()));
    assert_eq!(rows[1].vec3("position"), Some([1.25, -2.5, 3.75]));
    assert_eq!(
        parser.player_tracking_frames(&[], 0, 8, 1).unwrap()[0].tick,
        7
    );
    let error = parser
        .visit_scene_frames(&[], 0, 8, 1, false, |_| anyhow::bail!("consumer stopped"))
        .unwrap_err();
    assert_eq!(error.to_string(), "consumer stopped");
    let mut output = Vec::new();
    parser.write_match_state(&[], &mut output).unwrap();
    assert_eq!(output, b"fixture");
}
