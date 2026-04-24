use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub contact_type: String,
    pub first_name: String,
    pub last_name: String,
    pub full_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_suffix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phonetic_given_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phonetic_middle_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phonetic_family_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phonetic_organization_name: Option<String>,
    pub emails: Vec<String>,
    pub phones: Vec<String>,
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
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

    fn minimal_contact() -> Contact {
        Contact {
            id: "1".into(),
            contact_type: "person".into(),
            first_name: "Ada".into(),
            last_name: "Lovelace".into(),
            full_name: "Ada Lovelace".into(),
            name_prefix: None,
            middle_name: None,
            name_suffix: None,
            nickname: None,
            phonetic_given_name: None,
            phonetic_middle_name: None,
            phonetic_family_name: None,
            phonetic_organization_name: None,
            emails: vec![],
            phones: vec![],
            urls: vec![],
            organization: None,
            department: None,
            job_title: None,
            note: None,
            birthday: None,
            addresses: vec![],
        }
    }

    #[test]
    fn contact_omits_empty_optionals_when_serialized() {
        let c = minimal_contact();
        let json = serde_json::to_string(&c).unwrap();
        // skip_serializing_if should elide all Option::None scalars
        assert!(!json.contains("organization"));
        assert!(!json.contains("department"));
        assert!(!json.contains("job_title"));
        assert!(!json.contains("note"));
        assert!(!json.contains("birthday"));
        assert!(!json.contains("nickname"));
        assert!(!json.contains("middle_name"));
        assert!(!json.contains("name_prefix"));
        assert!(!json.contains("name_suffix"));
        assert!(!json.contains("phonetic"));
        // required fields remain
        assert!(json.contains("\"first_name\":\"Ada\""));
        assert!(json.contains("\"full_name\":\"Ada Lovelace\""));
        assert!(json.contains("\"contact_type\":\"person\""));
        assert!(json.contains("\"emails\":[]"));
        assert!(json.contains("\"urls\":[]"));
    }

    #[test]
    fn contact_includes_populated_optionals() {
        let mut c = minimal_contact();
        c.organization = Some("Analytical Engine Co.".into());
        c.job_title = Some("Mathematician".into());
        c.birthday = Some("1815-12-10".into());
        c.nickname = Some("Ada".into());
        c.name_prefix = Some("Lady".into());
        c.emails = vec!["ada@example.org".into()];
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"organization\":\"Analytical Engine Co.\""));
        assert!(json.contains("\"birthday\":\"1815-12-10\""));
        assert!(json.contains("\"nickname\":\"Ada\""));
        assert!(json.contains("\"name_prefix\":\"Lady\""));
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
