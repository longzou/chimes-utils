use rbatis::wrapper::Wrapper;
use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SearchField {
    pub field: Option<String>,
    pub operator: Option<String>,
    pub render: Option<String>,
    pub field_type: Option<String>,
    pub val: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CommonSearch {
    pub limit: Option<u64>,
    pub page: Option<u64>,
    pub order: Option<String>,
    pub search: Vec<SearchField>,
}

impl CommonSearch {
    fn to_value(&self, field: &SearchField) -> Option<rbatis::Value> {
        let field_value = field.val.clone().unwrap_or_default();
        self.to_value_inner(field, field_value)
    }

    fn to_value_inner(&self, field: &SearchField, field_value: String) -> Option<rbatis::Value> {
        let value = match field.field_type.clone() {
            Some(t) => {
                match t.to_lowercase().as_str() {
                    "integer" => match field_value.parse::<i64>() {
                        Ok(v) => Some(rbatis::Value::Int64(v)),
                        Err(_) => None,
                    },
                    "float" => match field_value.parse::<f64>() {
                        Ok(v) => Some(rbatis::Value::Double(v)),
                        Err(_) => None,
                    },
                    "decimal" => Some(rbatis::Value::String(field_value)),
                    "boolean" => Some(rbatis::Value::from(
                        field_value.to_lowercase().as_str() == "true",
                    )),
                    "datetime" => match rbatis::DateTimeNative::from_str(&field_value) {
                        Ok(_) => Some(rbatis::Value::from(&field_value)),
                        Err(_) => None,
                    },
                    "date" => match rbatis::DateNative::from_str(&field_value) {
                        Ok(_) => Some(rbatis::Value::from(&field_value)),
                        Err(_) => None,
                    },
                    _ => Some(rbatis::Value::String(field_value)), // default to string
                }
            }
            None => Some(rbatis::Value::String(field_value)),
        };
        value
    }

    fn split_value(&self, field: &SearchField) -> Vec<Option<rbatis::Value>> {
        let field_value = field.val.clone().unwrap_or_default();
        let list: Vec<Option<rbatis::Value>> = field_value
            .split(',')
            .map(|f| self.to_value_inner(field, f.to_string()))
            .collect();
        list
    }

    pub fn into_wrapper(&self, mut wp: Wrapper) -> Wrapper {
        for f in self.search.clone() {
            if let Some(t) = f.operator.clone() {
                match t.to_lowercase().as_str() {
                    "eq" => {
                        wp = wp.and().eq(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "ne" => {
                        wp = wp.and().ne(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "gt" => {
                        wp = wp.and().gt(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "egt" => {
                        wp = wp.and().gt(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "lt" => {
                        wp = wp.and().lt(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "elt" => {
                        wp = wp.and().le(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "like" => {
                        wp = wp.and().like(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "not like" => {
                        wp = wp.and().not_like(
                            f.field.clone().unwrap_or_default().as_str(),
                            self.to_value(&f),
                        );
                    }
                    "in" => {
                        let list = self.split_value(&f);
                        wp = wp
                            .and()
                            .r#in(f.field.clone().unwrap_or_default().as_str(), &list);
                    }
                    "not in" => {
                        let list = self.split_value(&f);
                        wp = wp
                            .and()
                            .r#not_in(f.field.clone().unwrap_or_default().as_str(), &list);
                    }
                    "range" => {
                        let list = self.split_value(&f);
                        wp = wp.and().between(
                            f.field.clone().unwrap_or_default().as_str(),
                            list[0].clone(),
                            list[1].clone(),
                        );
                    }
                    "not range" => {
                        let list = self.split_value(&f);
                        wp = wp.and().not_between(
                            f.field.clone().unwrap_or_default().as_str(),
                            list[0].clone(),
                            list[1].clone(),
                        );
                    }
                    "null" => {
                        wp = wp
                            .and()
                            .is_null(f.field.clone().unwrap_or_default().as_str());
                    }
                    "not null" => {
                        wp = wp
                            .and()
                            .is_not_null(f.field.clone().unwrap_or_default().as_str());
                    }
                    _ => {}
                }
            }
        }
        wp
    }
}
