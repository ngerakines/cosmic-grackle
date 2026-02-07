use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_contacts::*;
use objc2_foundation::*;
use tokio::sync::{mpsc, oneshot};

use crate::error::ContactsError;
use crate::keys::contact_fetch_keys_array;
use crate::models::{Contact, ContactGroup};

const NS_DATE_COMPONENT_UNDEFINED: isize = isize::MAX;

#[derive(Debug, Clone)]
pub struct ContactsCreateParams {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub organization: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ContactsUpdateParams {
    pub id: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub organization: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
}

enum StoreCommand {
    ListContacts {
        limit: Option<usize>,
        reply: oneshot::Sender<Result<Vec<Contact>, ContactsError>>,
    },
    SearchContacts {
        query: String,
        reply: oneshot::Sender<Result<Vec<Contact>, ContactsError>>,
    },
    GetContact {
        id: String,
        reply: oneshot::Sender<Result<Option<Contact>, ContactsError>>,
    },
    CreateContact {
        params: ContactsCreateParams,
        reply: oneshot::Sender<Result<String, ContactsError>>,
    },
    UpdateContact {
        params: ContactsUpdateParams,
        reply: oneshot::Sender<Result<bool, ContactsError>>,
    },
    DeleteContact {
        id: String,
        reply: oneshot::Sender<Result<bool, ContactsError>>,
    },
    ListGroups {
        reply: oneshot::Sender<Result<Vec<ContactGroup>, ContactsError>>,
    },
    GetGroupMembers {
        name: String,
        reply: oneshot::Sender<Result<Vec<Contact>, ContactsError>>,
    },
}

pub struct ContactStoreHandle {
    sender: mpsc::Sender<StoreCommand>,
    _thread: Arc<std::thread::JoinHandle<()>>,
}

impl ContactStoreHandle {
    pub fn new() -> Result<Self, ContactsError> {
        let (tx, mut rx) = mpsc::channel::<StoreCommand>(32);

        let handle = std::thread::spawn(move || {
            let store = unsafe { CNContactStore::new() };

            // Log the authorization status but don't block on it.
            // On macOS, CLI tools can't always present the authorization dialog.
            // The user may need to grant access in System Settings > Privacy & Security > Contacts.
            // We proceed regardless and let individual operations report errors.
            log_authorization_status(&store);

            while let Some(cmd) = rx.blocking_recv() {
                match cmd {
                    StoreCommand::ListContacts { limit, reply } => {
                        let _ = reply.send(list_contacts_impl(&store, limit));
                    }
                    StoreCommand::SearchContacts { query, reply } => {
                        let _ = reply.send(search_contacts_impl(&store, &query));
                    }
                    StoreCommand::GetContact { id, reply } => {
                        let _ = reply.send(get_contact_impl(&store, &id));
                    }
                    StoreCommand::CreateContact { params, reply } => {
                        let _ = reply.send(create_contact_impl(&store, &params));
                    }
                    StoreCommand::UpdateContact { params, reply } => {
                        let _ = reply.send(update_contact_impl(&store, &params));
                    }
                    StoreCommand::DeleteContact { id, reply } => {
                        let _ = reply.send(delete_contact_impl(&store, &id));
                    }
                    StoreCommand::ListGroups { reply } => {
                        let _ = reply.send(list_groups_impl(&store));
                    }
                    StoreCommand::GetGroupMembers { name, reply } => {
                        let _ = reply.send(get_group_members_impl(&store, &name));
                    }
                }
            }
        });

        Ok(Self {
            sender: tx,
            _thread: Arc::new(handle),
        })
    }

    pub async fn list_contacts(&self, limit: Option<usize>) -> Result<Vec<Contact>, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::ListContacts { limit, reply: tx })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn search_contacts(&self, query: &str) -> Result<Vec<Contact>, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::SearchContacts {
                query: query.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn get_contact(&self, id: &str) -> Result<Option<Contact>, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::GetContact {
                id: id.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn create_contact(
        &self,
        params: ContactsCreateParams,
    ) -> Result<String, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::CreateContact { params, reply: tx })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn update_contact(
        &self,
        params: ContactsUpdateParams,
    ) -> Result<bool, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::UpdateContact { params, reply: tx })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn delete_contact(&self, id: &str) -> Result<bool, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::DeleteContact {
                id: id.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn list_groups(&self) -> Result<Vec<ContactGroup>, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::ListGroups { reply: tx })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }

    pub async fn get_group_members(&self, name: &str) -> Result<Vec<Contact>, ContactsError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(StoreCommand::GetGroupMembers {
                name: name.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| ContactsError::ChannelError("store thread unavailable".into()))?;
        rx.await
            .map_err(|_| ContactsError::ChannelError("no response from store thread".into()))?
    }
}

fn log_authorization_status(store: &CNContactStore) {
    let status =
        unsafe { CNContactStore::authorizationStatusForEntityType(CNEntityType::Contacts) };

    match status {
        CNAuthorizationStatus::Authorized => {
            tracing::info!("Contacts access authorized");
        }
        CNAuthorizationStatus::NotDetermined => {
            tracing::info!("Contacts access not yet determined, requesting...");
            let (tx, rx) = std::sync::mpsc::channel();
            let block = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
                let _ = tx.send(granted.as_bool());
            });

            unsafe {
                store.requestAccessForEntityType_completionHandler(CNEntityType::Contacts, &block);
            }

            match rx.recv() {
                Ok(true) => {
                    tracing::info!("Contacts access granted");
                }
                Ok(false) => {
                    tracing::warn!(
                        "Contacts access denied. Grant access in System Settings > Privacy & Security > Contacts for your terminal application"
                    );
                }
                Err(_) => {
                    tracing::warn!("Authorization callback did not fire");
                }
            }
        }
        CNAuthorizationStatus::Denied => {
            tracing::warn!(
                "Contacts access denied. Grant access in System Settings > Privacy & Security > Contacts for your terminal application"
            );
        }
        CNAuthorizationStatus::Restricted => {
            tracing::warn!("Contacts access restricted by system policy");
        }
        _ => {
            tracing::info!("Contacts authorization status: {:?}, proceeding", status);
        }
    }
}

fn cncontact_to_model(contact: &CNContact) -> Contact {
    unsafe {
        let id = contact.identifier().to_string();
        let first_name = contact.givenName().to_string();
        let last_name = contact.familyName().to_string();
        let org = contact.organizationName().to_string();
        let job = contact.jobTitle().to_string();

        let emails: Vec<String> = {
            let labeled_values = contact.emailAddresses();
            let count = labeled_values.len();
            (0..count)
                .map(|i| {
                    let lv = labeled_values.objectAtIndex(i);
                    lv.value().to_string()
                })
                .collect()
        };

        let phones: Vec<String> = {
            let labeled_values = contact.phoneNumbers();
            let count = labeled_values.len();
            (0..count)
                .map(|i| {
                    let lv = labeled_values.objectAtIndex(i);
                    lv.value().stringValue().to_string()
                })
                .collect()
        };

        let addresses: Vec<String> = {
            let labeled_values = contact.postalAddresses();
            let count = labeled_values.len();
            (0..count)
                .map(|i| {
                    let lv = labeled_values.objectAtIndex(i);
                    let addr = lv.value();
                    let street = addr.street().to_string();
                    let city = addr.city().to_string();
                    let state = addr.state().to_string();
                    let postal_code = addr.postalCode().to_string();
                    let country = addr.country().to_string();
                    let parts: Vec<&str> = [
                        street.as_str(),
                        city.as_str(),
                        state.as_str(),
                        postal_code.as_str(),
                        country.as_str(),
                    ]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                    parts.join(", ")
                })
                .collect()
        };

        let birthday = contact.birthday().map(|bday| {
            let year = bday.year();
            let month = bday.month();
            let day = bday.day();
            if year == NS_DATE_COMPONENT_UNDEFINED {
                format!("--{:02}-{:02}", month, day)
            } else {
                format!("{:04}-{:02}-{:02}", year, month, day)
            }
        });

        let full_name = match (first_name.is_empty(), last_name.is_empty()) {
            (false, false) => format!("{} {}", first_name, last_name),
            (false, true) => first_name.clone(),
            (true, false) => last_name.clone(),
            (true, true) => org.clone(),
        };

        Contact {
            id,
            first_name,
            last_name,
            full_name,
            emails,
            phones,
            organization: if org.is_empty() { None } else { Some(org) },
            job_title: if job.is_empty() { None } else { Some(job) },
            // note is not fetched — requires com.apple.developer.contacts.notes entitlement
            note: None,
            birthday,
            addresses,
        }
    }
}

fn enumerate_contacts(
    store: &CNContactStore,
    request: &CNContactFetchRequest,
    limit: Option<usize>,
) -> Result<Vec<Contact>, ContactsError> {
    let contacts = Rc::new(RefCell::new(Vec::new()));
    let contacts_clone = Rc::clone(&contacts);
    let limit_val = limit.unwrap_or(usize::MAX);

    let block = RcBlock::new(
        move |contact_ptr: NonNull<CNContact>, stop: NonNull<Bool>| {
            let contact_ref = unsafe { contact_ptr.as_ref() };
            let mut vec = contacts_clone.borrow_mut();
            vec.push(cncontact_to_model(contact_ref));
            if vec.len() >= limit_val {
                unsafe { stop.as_ptr().write(Bool::YES) };
            }
        },
    );

    let mut error: Option<Retained<NSError>> = None;
    let success = unsafe {
        store.enumerateContactsWithFetchRequest_error_usingBlock(request, Some(&mut error), &block)
    };

    if !success {
        if let Some(err) = error {
            return Err(ContactsError::ObjcError(
                err.localizedDescription().to_string(),
            ));
        }
        return Err(ContactsError::OperationFailed(
            "enumerate contacts failed".into(),
        ));
    }

    drop(block);
    Ok(Rc::try_unwrap(contacts).unwrap().into_inner())
}

fn list_contacts_impl(
    store: &CNContactStore,
    limit: Option<usize>,
) -> Result<Vec<Contact>, ContactsError> {
    let keys = contact_fetch_keys_array();
    let request = unsafe {
        CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys)
    };
    enumerate_contacts(store, &request, limit)
}

fn search_contacts_impl(
    store: &CNContactStore,
    query: &str,
) -> Result<Vec<Contact>, ContactsError> {
    let keys = contact_fetch_keys_array();
    let ns_query = NSString::from_str(query);
    let predicate = unsafe { CNContact::predicateForContactsMatchingName(&ns_query) };

    let request = unsafe {
        CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys)
    };
    unsafe { request.setPredicate(Some(&predicate)) };

    enumerate_contacts(store, &request, None)
}

fn get_contact_impl(store: &CNContactStore, id: &str) -> Result<Option<Contact>, ContactsError> {
    let keys = contact_fetch_keys_array();
    let ns_id = NSString::from_str(id);

    match unsafe { store.unifiedContactWithIdentifier_keysToFetch_error(&ns_id, &keys) } {
        Ok(contact) => Ok(Some(cncontact_to_model(&contact))),
        Err(err) => {
            let desc = err.localizedDescription().to_string();
            if desc.contains("not found") || desc.contains("No results") {
                Ok(None)
            } else {
                Err(ContactsError::ObjcError(desc))
            }
        }
    }
}

fn create_contact_impl(
    store: &CNContactStore,
    params: &ContactsCreateParams,
) -> Result<String, ContactsError> {
    unsafe {
        let contact = CNMutableContact::new();

        if let Some(ref first) = params.first_name {
            contact.setGivenName(&NSString::from_str(first));
        }
        if let Some(ref last) = params.last_name {
            contact.setFamilyName(&NSString::from_str(last));
        }
        if let Some(ref org) = params.organization {
            contact.setOrganizationName(&NSString::from_str(org));
        }
        if let Some(ref title) = params.job_title {
            contact.setJobTitle(&NSString::from_str(title));
        }
        // note is not set — requires com.apple.developer.contacts.notes entitlement

        if let Some(ref email) = params.email {
            let email_value = NSString::from_str(email);
            let label = NSString::from_str("work");
            let labeled = CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*email_value);
            let emails = NSArray::from_retained_slice(&[labeled]);
            contact.setEmailAddresses(&emails);
        }

        if let Some(ref phone) = params.phone {
            if let Some(phone_number) =
                CNPhoneNumber::phoneNumberWithStringValue(&NSString::from_str(phone))
            {
                let label = NSString::from_str("mobile");
                let labeled =
                    CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*phone_number);
                let phones = NSArray::from_retained_slice(&[labeled]);
                contact.setPhoneNumbers(&phones);
            }
        }

        let save_request = CNSaveRequest::new();
        save_request.addContact_toContainerWithIdentifier(&contact, None);

        store
            .executeSaveRequest_error(&save_request)
            .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;

        Ok(contact.identifier().to_string())
    }
}

fn update_contact_impl(
    store: &CNContactStore,
    params: &ContactsUpdateParams,
) -> Result<bool, ContactsError> {
    let keys = contact_fetch_keys_array();
    let ns_id = NSString::from_str(&params.id);

    let contact = unsafe { store.unifiedContactWithIdentifier_keysToFetch_error(&ns_id, &keys) }
        .map_err(|err| ContactsError::ContactNotFound(err.localizedDescription().to_string()))?;

    unsafe {
        let mutable: Retained<CNMutableContact> = contact.mutableCopy();

        if let Some(ref first) = params.first_name {
            mutable.setGivenName(&NSString::from_str(first));
        }
        if let Some(ref last) = params.last_name {
            mutable.setFamilyName(&NSString::from_str(last));
        }
        if let Some(ref org) = params.organization {
            mutable.setOrganizationName(&NSString::from_str(org));
        }
        if let Some(ref title) = params.job_title {
            mutable.setJobTitle(&NSString::from_str(title));
        }
        if let Some(ref note) = params.note {
            mutable.setNote(&NSString::from_str(note));
        }

        let save_request = CNSaveRequest::new();
        save_request.updateContact(&mutable);

        store
            .executeSaveRequest_error(&save_request)
            .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;
    }

    Ok(true)
}

fn delete_contact_impl(store: &CNContactStore, id: &str) -> Result<bool, ContactsError> {
    let keys = contact_fetch_keys_array();
    let ns_id = NSString::from_str(id);

    let contact = unsafe { store.unifiedContactWithIdentifier_keysToFetch_error(&ns_id, &keys) }
        .map_err(|err| ContactsError::ContactNotFound(err.localizedDescription().to_string()))?;

    unsafe {
        let mutable: Retained<CNMutableContact> = contact.mutableCopy();

        let save_request = CNSaveRequest::new();
        save_request.deleteContact(&mutable);

        store
            .executeSaveRequest_error(&save_request)
            .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;
    }

    Ok(true)
}

fn list_groups_impl(store: &CNContactStore) -> Result<Vec<ContactGroup>, ContactsError> {
    let groups = unsafe { store.groupsMatchingPredicate_error(None) }
        .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;

    let keys = contact_fetch_keys_array();
    let mut result = Vec::new();

    let count = groups.len();
    for i in 0..count {
        let group = groups.objectAtIndex(i);
        let group_id = unsafe { group.identifier() }.to_string();
        let group_name = unsafe { group.name() }.to_string();

        // Count members by enumerating
        let predicate = unsafe {
            CNContact::predicateForContactsInGroupWithIdentifier(&NSString::from_str(&group_id))
        };
        let request = unsafe {
            CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys)
        };
        unsafe { request.setPredicate(Some(&predicate)) };

        let member_count = Rc::new(RefCell::new(0usize));
        let count_clone = Rc::clone(&member_count);

        let block = RcBlock::new(move |_contact: NonNull<CNContact>, _stop: NonNull<Bool>| {
            *count_clone.borrow_mut() += 1;
        });

        let mut error: Option<Retained<NSError>> = None;
        unsafe {
            store.enumerateContactsWithFetchRequest_error_usingBlock(
                &request,
                Some(&mut error),
                &block,
            );
        }

        drop(block);
        let count_val = *member_count.borrow();

        result.push(ContactGroup {
            id: group_id,
            name: group_name,
            member_count: count_val,
        });
    }

    Ok(result)
}

fn get_group_members_impl(
    store: &CNContactStore,
    name: &str,
) -> Result<Vec<Contact>, ContactsError> {
    let groups = unsafe { store.groupsMatchingPredicate_error(None) }
        .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;

    let mut target_group_id = None;
    let count = groups.len();
    for i in 0..count {
        let group = groups.objectAtIndex(i);
        let group_name = unsafe { group.name() }.to_string();
        if group_name.eq_ignore_ascii_case(name) {
            target_group_id = Some(unsafe { group.identifier() }.to_string());
            break;
        }
    }

    let group_id = target_group_id.ok_or_else(|| ContactsError::GroupNotFound(name.into()))?;

    let keys = contact_fetch_keys_array();
    let predicate = unsafe {
        CNContact::predicateForContactsInGroupWithIdentifier(&NSString::from_str(&group_id))
    };
    let request = unsafe {
        CNContactFetchRequest::initWithKeysToFetch(CNContactFetchRequest::alloc(), &keys)
    };
    unsafe { request.setPredicate(Some(&predicate)) };

    enumerate_contacts(store, &request, None)
}
