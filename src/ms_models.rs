use std::collections::HashMap;

use serde::Serialize;

#[derive(Serialize)]
pub struct MsMeme {
    pub id: i32,
    pub text: Option<String>,
    pub translations: HashMap<String, MsMemeTranslation>,
}

#[derive(Serialize)]
pub struct MsMemeTranslation {
    pub title: String,
    pub caption: String,
    pub description: String,
}
