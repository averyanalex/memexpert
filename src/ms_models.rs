use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct MsMeme {
    pub id: i32,
    pub text: Option<String>,
    pub translations: HashMap<String, MsMemeTranslation>,
}

#[derive(Debug, Deserialize)]
pub struct MsMemeResult {
    pub id: i32,
}

#[derive(Serialize)]
pub struct MsMemeTranslation {
    pub title: String,
    pub caption: String,
    pub description: String,
}
