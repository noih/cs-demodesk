//! Lossless state changes. Only the preceding state is retained, never the full capture.
use anyhow::{ensure, Result};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, io::Write};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Changes<'a> {
    tick: i32,
    net_tick: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    create: Vec<&'a Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remove: Vec<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    set: Vec<(i32, u32, &'a Value)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unset: Vec<(i32, u32)>,
}
pub struct Journal<W: Write> {
    output: W,
    previous: BTreeMap<i32, Value>,
    fields: BTreeMap<Vec<String>, u32>,
    last_tick: Option<i32>,
    frames: usize,
}
impl<W: Write> Journal<W> {
    pub fn new(mut output: W, source: super::Source, tracking_only: bool) -> Result<Self> {
        serde_json::to_writer(
            &mut output,
            &super::Artifact {
                contract: super::Contract {
                    module: "scene-journal".into(),
                    schema_version: 1,
                    implementation_version: "0.1.0".into(),
                },
                source,
                dependencies: vec![],
                data: serde_json::json!({"profile":if tracking_only {"player-tracking"} else {"scene"}}),
            },
        )?;
        output.write_all(b"\n")?;
        Ok(Self {
            output,
            previous: BTreeMap::new(),
            fields: BTreeMap::new(),
            last_tick: None,
            frames: 0,
        })
    }
    pub fn push(&mut self, frame: super::scene::SceneFrame) -> Result<()> {
        ensure!(
            self.last_tick.is_none_or(|tick| frame.tick > tick),
            "non-monotonic journal tick"
        );
        let current = frame
            .entities
            .into_iter()
            .map(|e| Ok((e.entity_id, serde_json::to_value(e)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut changes = Changes {
            tick: frame.tick,
            net_tick: frame.net_tick,
            fields: vec![],
            create: vec![],
            remove: vec![],
            set: vec![],
            unset: vec![],
        };
        for id in self.previous.keys() {
            if !current.contains_key(id) {
                changes.remove.push(*id);
            }
        }
        for (id, value) in &current {
            match self.previous.get(id) {
                Some(old)
                    if old["serial"] == value["serial"]
                        && old["className"] == value["className"] =>
                {
                    diff(old, value, &mut vec![], &mut |path, value| {
                        let next = self.fields.len() as u32;
                        let field = *self.fields.entry(path.to_vec()).or_insert_with(|| {
                            changes.fields.push(path.to_vec());
                            next
                        });
                        if let Some(value) = value {
                            changes.set.push((*id, field, value));
                        } else {
                            changes.unset.push((*id, field));
                        }
                    });
                }
                _ => changes.create.push(value),
            }
        }
        serde_json::to_writer(&mut self.output, &changes)?;
        self.output.write_all(b"\n")?;
        self.previous = current;
        self.last_tick = Some(frame.tick);
        self.frames += 1;
        Ok(())
    }
    pub fn finish(mut self) -> Result<usize> {
        serde_json::to_writer(
            &mut self.output,
            &serde_json::json!({"end":self.frames,"lastTick":self.last_tick}),
        )?;
        self.output.write_all(b"\n")?;
        self.output.flush()?;
        Ok(self.frames)
    }
}
fn diff<'a>(
    old: &Value,
    new: &'a Value,
    path: &mut Vec<String>,
    emit: &mut impl FnMut(&[String], Option<&'a Value>),
) {
    if old == new {
        return;
    }
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys() {
                if !b.contains_key(key) {
                    path.push(key.clone());
                    emit(path, None);
                    path.pop();
                }
            }
            for (key, value) in b {
                path.push(key.clone());
                if let Some(before) = a.get(key) {
                    diff(before, value, path, emit);
                } else {
                    emit(path, Some(value));
                }
                path.pop();
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (i, (before, value)) in a.iter().zip(b).enumerate() {
                path.push(i.to_string());
                diff(before, value, path, emit);
                path.pop();
            }
        }
        _ => emit(path, Some(new)),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changes_preserve_null_array_edits_and_removals_without_repeating_constants() {
        let old = serde_json::json!({"static":"unchanged","bytes":[0,0,0],"gone":42,"nullable":0});
        let new = serde_json::json!({"static":"unchanged","bytes":[0,7,0],"nullable":null});
        let mut ops = vec![];
        diff(&old, &new, &mut vec![], &mut |path, value| {
            ops.push((path.to_vec(), value.cloned()))
        });
        assert_eq!(ops.len(), 3);
        assert!(ops.contains(&(vec!["bytes".into(), "1".into()], Some(serde_json::json!(7)))));
        assert!(ops.contains(&(vec!["gone".into()], None)));
        assert!(ops.contains(&(vec!["nullable".into()], Some(Value::Null))));
    }
}
