//! Diagnostic only: inspect measured aim around all kill events in one native scan.
use anyhow::{ensure, Result};
use demodesk_core::analysis::native_body;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path};
fn main() -> Result<()> {
    let a: Vec<_> = std::env::args().skip(1).collect();
    ensure!(a.len()==6, "expected STATE PARSED GAME VRF CACHE NEW_OUTPUT");
    let parsed: Value = serde_json::from_slice(&std::fs::read(&a[1])?)?;
    let kills=parsed["kills"].as_array().unwrap();
    let mut windows = BTreeMap::<i32,Vec<usize>>::new();
    for (i,kill) in kills.iter().enumerate() {
        let tick=kill["tick"].as_i64().unwrap() as i32;
        for t in (tick-16).max(0)..=tick+3 { windows.entry(t).or_default().push(i); }
    }
    let prepared=native_body::prepare(Path::new(&a[0]),Path::new(&a[4]),Path::new(&a[2]),Path::new(&a[3]))?;
    let mut samples=vec![Vec::<Value>::new();kills.len()];
    let coverage=native_body::visit(Path::new(&a[0]),&prepared,&prepared.skeleton,|tick,frame| {
        if let Some(indices)=windows.get(&tick) {
            for &i in indices {
                let kill=&kills[i];
                let attacker=frame.iter().find(|p|Some(p.player_id.as_str())==kill["attacker"]["steamid"].as_str());
                let victim=frame.iter().find(|p|Some(p.player_id.as_str())==kill["victim"]["steamid"].as_str());
                if let Some(p)=attacker {
                    samples[i].push(json!({"tick":tick,"eye":p.eye,"view":p.view,"targetEye":victim.and_then(|q|q.eye),"points":victim.map(|q| &q.points)}));
                }
            }
        }
        Ok(())
    })?;
    ensure!(samples.iter().any(|s| !s.is_empty()), "no kill participants matched the native player IDs");
    let output=std::fs::OpenOptions::new().write(true).create_new(true).open(&a[5])?;
    serde_json::to_writer(output,&json!({"pointNames":prepared.point_names,"coverage":coverage,"kills":kills,"samples":samples}))?;
    println!("saved {} kill windows",kills.len());
    Ok(())
}
