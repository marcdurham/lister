use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: Uuid,
    pub text: String,
    pub done: bool,
}

impl Item {
    pub fn new(text: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            text,
            done: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ListState {
    pub items: Vec<Item>,
}
