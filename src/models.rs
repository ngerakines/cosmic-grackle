use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub first_name: String,
    pub last_name: String,
    pub full_name: String,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<String>,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactGroup {
    pub id: String,
    pub name: String,
    pub member_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_omits_empty_optionals_when_serialized() {
        let c = Contact {
            id: "1".into(),
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            full_name: "Ada Lovelace".into(),
            emails: vec![],
            phones: vec![],
            organization: None,
            job_title: None,
            note: None,
            birthday: None,
            addresses: vec![],
        };
        let json = serde_json::to_string(&c).unwrap();
        // skip_serializing_if should elide these four fields
        assert!(!json.contains("organization"));
        assert!(!json.contains("job_title"));
        assert!(!json.contains("note"));
        assert!(!json.contains("birthday"));
        // required fields remain
        assert!(json.contains("\"first_name\":\"Ada\""));
        assert!(json.contains("\"full_name\":\"Ada Lovelace\""));
        assert!(json.contains("\"emails\":[]"));
    }

    #[test]
    fn contact_includes_populated_optionals() {
        let c = Contact {
            id: "1".into(),
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            full_name: "Ada Lovelace".into(),
            emails: vec!["ada@example.org".into()],
            phones: vec![],
            organization: Some("Analytical Engine Co.".into()),
            job_title: Some("Mathematician".into()),
            note: None,
            birthday: Some("1815-12-10".into()),
            addresses: vec![],
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"organization\":\"Analytical Engine Co.\""));
        assert!(json.contains("\"birthday\":\"1815-12-10\""));
        assert!(!json.contains("\"note\""));
    }

    #[test]
    fn contact_group_roundtrips_through_json() {
        let g = ContactGroup {
            id: "grp-1".into(),
            name: "Family".into(),
            member_count: 7,
        };
        let json = serde_json::to_string(&g).unwrap();
        let back: ContactGroup = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "grp-1");
        assert_eq!(back.name, "Family");
        assert_eq!(back.member_count, 7);
    }
}
