use objc2::runtime::ProtocolObject;
use objc2_contacts::CNKeyDescriptor;
use objc2_foundation::{NSArray, NSString};

pub fn contact_fetch_keys() -> Vec<objc2::rc::Retained<ProtocolObject<dyn CNKeyDescriptor>>> {
    let key_strings = [
        "identifier",
        "givenName",
        "familyName",
        "organizationName",
        "jobTitle",
        // "note" is omitted — requires com.apple.developer.contacts.notes entitlement
        // on macOS Sequoia+. Accessing it without the entitlement throws an uncatchable
        // ObjC exception that aborts the process.
        "emailAddresses",
        "phoneNumbers",
        "postalAddresses",
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
