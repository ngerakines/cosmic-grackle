use objc2::runtime::ProtocolObject;
use objc2_contacts::CNKeyDescriptor;
use objc2_foundation::{NSArray, NSString};

pub fn contact_fetch_keys() -> Vec<objc2::rc::Retained<ProtocolObject<dyn CNKeyDescriptor>>> {
    let key_strings = [
        "identifier",
        "contactType",
        "namePrefix",
        "givenName",
        "middleName",
        "familyName",
        "nameSuffix",
        "nickname",
        "organizationName",
        "departmentName",
        "jobTitle",
        "phoneticGivenName",
        "phoneticMiddleName",
        "phoneticFamilyName",
        "phoneticOrganizationName",
        // "note" is omitted — reading it requires com.apple.developer.contacts.notes
        // entitlement on macOS Sequoia+. Accessing without the entitlement throws an
        // uncatchable ObjC exception that aborts the process. Writes via setNote do
        // not require the entitlement (the create/update paths set it directly).
        "emailAddresses",
        "phoneNumbers",
        "postalAddresses",
        "urlAddresses",
        "birthday",
    ];

    key_strings
        .into_iter()
        .map(|s| {
            let ns_string = NSString::from_str(s);
            ProtocolObject::from_retained(ns_string)
        })
        .collect()
}

pub fn contact_fetch_keys_array()
-> objc2::rc::Retained<NSArray<ProtocolObject<dyn CNKeyDescriptor>>> {
    let keys = contact_fetch_keys();
    let refs: Vec<&ProtocolObject<dyn CNKeyDescriptor>> = keys.iter().map(|k| k.as_ref()).collect();
    NSArray::from_slice(&refs)
}
