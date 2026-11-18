use std::collections::HashMap;
#[derive(Debug, Clone)]
pub enum Column {
    Bool(Vec<Option<bool>>),
    U32(Vec<Option<u32>>),
    I32(Vec<Option<i32>>),
    F32(Vec<Option<f32>>),
    U64(Vec<Option<u64>>),
    String(Vec<Option<String>>),
    StringVec(Vec<Vec<String>>),
    XYZVec(Vec<Option<[f32; 3]>>),
}
/// Column-major result of a tick / projectile pass, taken straight out of the
/// parser's data frame (no per-row allocation). Rows are visited in tick order.
pub struct Rows {
    cols: Vec<Column>,
    index: HashMap<String, usize>,
    /// row indices sorted by tick
    order: Vec<usize>,
}

/// One row of [`Rows`]; accessors tolerate the parser's variant choices.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    rows: &'a Rows,
    i: usize,
}

impl Rows {
    pub fn from_columns(columns: Vec<(String, Column)>, len: usize) -> Self {
        let mut index = HashMap::new();
        let cols = columns
            .into_iter()
            .enumerate()
            .map(|(i, (name, value))| {
                index.insert(name, i);
                value
            })
            .collect();
        let mut rows = Self {
            cols,
            index,
            order: (0..len).collect(),
        };
        let ticks: Vec<i32> = rows.iter().map(|r| r.tick().unwrap_or(0)).collect();
        rows.order.sort_by_key(|i| ticks[*i]);
        rows
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = Row<'_>> {
        (0..self.order.len()).map(move |i| Row { rows: self, i })
    }
}

pub(crate) fn col_len(v: &Column) -> usize {
    match v {
        Column::Bool(v) => v.len(),
        Column::U32(v) => v.len(),
        Column::I32(v) => v.len(),
        Column::F32(v) => v.len(),
        Column::U64(v) => v.len(),
        Column::String(v) => v.len(),
        Column::StringVec(v) => v.len(),
        Column::XYZVec(v) => v.len(),
    }
}

impl<'a> Row<'a> {
    fn col(&self, key: &str) -> Option<(&'a Column, usize)> {
        let col = &self.rows.cols[*self.rows.index.get(key)?];
        Some((col, self.rows.order[self.i]))
    }

    pub fn num(&self, key: &str) -> Option<f64> {
        let (col, i) = self.col(key)?;
        match col {
            Column::F32(v) => v.get(i).copied().flatten().map(f64::from),
            Column::I32(v) => v.get(i).copied().flatten().map(f64::from),
            Column::U32(v) => v.get(i).copied().flatten().map(f64::from),
            Column::U64(v) => v.get(i).copied().flatten().map(|v| v as f64),
            Column::Bool(v) => v.get(i).copied().flatten().map(|b| b as i32 as f64),
            _ => None,
        }
    }

    /// Missing vectors stay missing; do not replace absent positions with zero.
    pub fn vec3(&self, key: &str) -> Option<[f64; 3]> {
        match self.col(key)? {
            (Column::XYZVec(v), i) => v.get(i).copied().flatten().map(|p| p.map(f64::from)),
            _ => None,
        }
    }
    pub fn flag(&self, key: &str) -> bool {
        self.num(key).map(|v| v != 0.0).unwrap_or(false)
    }

    pub fn str(&self, key: &str) -> Option<&'a str> {
        match self.col(key)? {
            (Column::String(v), i) => v.get(i)?.as_deref(),
            _ => None,
        }
    }

    pub fn strs(&self, key: &str) -> Option<&'a [String]> {
        match self.col(key)? {
            (Column::StringVec(v), i) => v.get(i).map(Vec::as_slice),
            _ => None,
        }
    }

    pub fn tick(&self) -> Option<i32> {
        match self.col("tick")? {
            (Column::I32(v), i) => v.get(i).copied().flatten(),
            _ => None,
        }
    }

    /// Steamid as the parser stored it (u64 or string); bots carry `0`.
    pub fn steamid(&self) -> Option<String> {
        match self.col("steamid")? {
            (Column::U64(v), i) => v.get(i).copied().flatten().map(|v| v.to_string()),
            (Column::String(v), i) => v.get(i)?.clone(),
            _ => None,
        }
    }
}
