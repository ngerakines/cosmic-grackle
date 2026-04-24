//! macOS Contacts-framework bindings.
//!
//! Safety notes shared by the `unsafe` blocks in this module:
//!
//! - `CNContactStore` is not thread-safe. A single dedicated worker thread owns
//!   the store and is the only place it is touched; this is what makes the many
//!   `unsafe` calls in `*_impl` functions sound.
//! - Retained<T> values returned by objc2 accessors (identifier, givenName, etc.)
//!   are autoreleased-then-retained NSObject wrappers and are safe to use as long
//!   as Rust owns them. Reads into `String` via `.to_string()` copy out of the
//!   NSString so the Retained<NSString> can be dropped immediately after.
//! - Setter calls (`setGivenName`, `setFamilyName`, …) on `CNMutableContact` are
//!   marked unsafe by objc2 because they send selectors; the receiver in every
//!   call here is a freshly constructed or just-copied `CNMutableContact`, so
//!   the receiver-type invariant is trivially satisfied.
//! - Individual `unsafe` blocks with non-trivial invariants (block callbacks,
//!   framework statics, NonNull dereferences) carry their own SAFETY comments.

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

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

const AUTH_CALLBACK_TIMEOUT: Duration = Duration::from_secs(10);

const NS_DATE_COMPONENT_UNDEFINED: isize = isize::MAX;

#[derive(Debug, Clone, Default)]
pub struct PostalAddressData {
    pub street: Option<String>,
    pub city: Option<String>,
    pub state: Option<String>,
    pub postal_code: Option<String>,
    pub country: Option<String>,
}

impl PostalAddressData {
    pub fn is_empty(&self) -> bool {
        self.street.is_none()
            && self.city.is_none()
            && self.state.is_none()
            && self.postal_code.is_none()
            && self.country.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContactsCreateParams {
    pub contact_type: Option<String>,
    pub name_prefix: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub name_suffix: Option<String>,
    pub nickname: Option<String>,
    pub phonetic_given_name: Option<String>,
    pub phonetic_middle_name: Option<String>,
    pub phonetic_family_name: Option<String>,
    pub phonetic_organization_name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub url: Option<String>,
    pub organization: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
    pub birthday: Option<String>,
    pub postal_address: Option<PostalAddressData>,
}

#[derive(Debug, Clone, Default)]
pub struct ContactsUpdateParams {
    pub id: String,
    pub contact_type: Option<String>,
    pub name_prefix: Option<String>,
    pub first_name: Option<String>,
    pub middle_name: Option<String>,
    pub last_name: Option<String>,
    pub name_suffix: Option<String>,
    pub nickname: Option<String>,
    pub phonetic_given_name: Option<String>,
    pub phonetic_middle_name: Option<String>,
    pub phonetic_family_name: Option<String>,
    pub phonetic_organization_name: Option<String>,
    pub organization: Option<String>,
    pub department: Option<String>,
    pub job_title: Option<String>,
    pub note: Option<String>,
    pub birthday: Option<String>,
    pub phone_op: Option<PhoneOp>,
    pub email_op: Option<EmailOp>,
    pub url_op: Option<UrlOp>,
    pub postal_op: Option<PostalOp>,
}

#[derive(Debug, Clone)]
pub enum PhoneOp {
    Add(String),
    Remove(String),
    Replace { from: String, to: String },
}

#[derive(Debug, Clone)]
pub enum EmailOp {
    Add(String),
    Remove(String),
    Replace { from: String, to: String },
}

#[derive(Debug, Clone)]
pub enum UrlOp {
    Add(String),
    Remove(String),
    Replace { from: String, to: String },
}

/// Postal address update operation. Matches existing entries on this contact by
/// the joined-string form (same form exposed in `Contact.addresses`).
#[derive(Debug, Clone)]
pub enum PostalOp {
    Add(PostalAddressData),
    Remove(String),
    Replace { from: String, to: PostalAddressData },
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
            // SAFETY: CNContactStore must be created and used on a single thread because
            // the underlying Apple framework is not thread-safe. This thread owns the store
            // for its entire lifetime and is the only thread that touches it.
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

            match rx.recv_timeout(AUTH_CALLBACK_TIMEOUT) {
                Ok(true) => {
                    tracing::info!("Contacts access granted");
                }
                Ok(false) => {
                    tracing::warn!(
                        "Contacts access denied. Grant access in System Settings > Privacy & Security > Contacts for your terminal application"
                    );
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    tracing::warn!(
                        "Authorization prompt timed out after {:?}. If no dialog appeared, grant access in System Settings > Privacy & Security > Contacts and restart",
                        AUTH_CALLBACK_TIMEOUT
                    );
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    tracing::warn!("Authorization callback channel disconnected");
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

fn nserror_is_record_not_found(err: &NSError) -> bool {
    // SAFETY: reading an Objective-C `extern static` is `unsafe` by rule, but
    // `CNErrorDomain` is a framework-provided constant initialized before `main`.
    // We only read the pointer; the NSString behind it stays live for the process.
    let Some(expected) = (unsafe { CNErrorDomain }) else {
        return false;
    };
    err.domain().to_string() == expected.to_string()
        && err.code() == CNErrorCode::RecordDoesNotExist.0
}

fn none_if_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

fn format_postal_address(addr: &CNPostalAddress) -> String {
    // SAFETY: `addr` is a live CNPostalAddress; all getters below are non-mutating
    // NSString accessors that return autoreleased strings which we immediately copy.
    let (street, city, state, postal_code, country) = unsafe {
        (
            addr.street().to_string(),
            addr.city().to_string(),
            addr.state().to_string(),
            addr.postalCode().to_string(),
            addr.country().to_string(),
        )
    };
    [street, city, state, postal_code, country]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

fn cncontact_to_model(contact: &CNContact) -> Contact {
    // SAFETY: `contact` is a live CNContact owned by the caller. All property accessors
    // below are non-mutating and return Retained<NSObject> values, which are safe to use
    // for the duration of this function. The requested keys are fetched with the
    // descriptor array from `contact_fetch_keys`, so these reads will not trigger a
    // CNPropertyNotFetched exception.
    unsafe {
        let id = contact.identifier().to_string();
        let contact_type = match contact.contactType() {
            CNContactType::Organization => "organization".to_string(),
            _ => "person".to_string(),
        };
        let name_prefix = contact.namePrefix().to_string();
        let first_name = contact.givenName().to_string();
        let middle_name = contact.middleName().to_string();
        let last_name = contact.familyName().to_string();
        let name_suffix = contact.nameSuffix().to_string();
        let nickname = contact.nickname().to_string();
        let org = contact.organizationName().to_string();
        let department = contact.departmentName().to_string();
        let job = contact.jobTitle().to_string();
        let phonetic_given = contact.phoneticGivenName().to_string();
        let phonetic_middle = contact.phoneticMiddleName().to_string();
        let phonetic_family = contact.phoneticFamilyName().to_string();
        let phonetic_org = contact.phoneticOrganizationName().to_string();

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

        let urls: Vec<String> = {
            let labeled_values = contact.urlAddresses();
            let count = labeled_values.len();
            (0..count)
                .map(|i| {
                    let lv = labeled_values.objectAtIndex(i);
                    lv.value().to_string()
                })
                .collect()
        };

        let addresses: Vec<String> = {
            let labeled_values = contact.postalAddresses();
            let count = labeled_values.len();
            (0..count)
                .map(|i| {
                    let lv = labeled_values.objectAtIndex(i);
                    format_postal_address(&lv.value())
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
            contact_type,
            first_name,
            last_name,
            full_name,
            name_prefix: none_if_empty(name_prefix),
            middle_name: none_if_empty(middle_name),
            name_suffix: none_if_empty(name_suffix),
            nickname: none_if_empty(nickname),
            phonetic_given_name: none_if_empty(phonetic_given),
            phonetic_middle_name: none_if_empty(phonetic_middle),
            phonetic_family_name: none_if_empty(phonetic_family),
            phonetic_organization_name: none_if_empty(phonetic_org),
            emails,
            phones,
            urls,
            organization: none_if_empty(org),
            department: none_if_empty(department),
            job_title: none_if_empty(job),
            // note is not fetched — reading requires com.apple.developer.contacts.notes
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
            // SAFETY: CNContactStore guarantees a valid CNContact pointer for the
            // duration of this callback. NonNull is non-null by construction; aliasing
            // is fine because we only read from the contact here.
            let contact_ref = unsafe { contact_ptr.as_ref() };
            let mut vec = contacts_clone.borrow_mut();
            vec.push(cncontact_to_model(contact_ref));
            if vec.len() >= limit_val {
                // SAFETY: `stop` points to a BOOL owned by the framework for this
                // enumeration. Writing YES asks it to stop iterating; the framework
                // keeps the pointer valid until the callback returns.
                unsafe { stop.as_ptr().write(Bool::YES) };
            }
        },
    );

    let mut error: Option<Retained<NSError>> = None;
    // SAFETY: The fetch request and block outlive the call. The block only borrows
    // `contacts_clone` through an Rc, which is dropped when the block is dropped below.
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

    // Drop the block first so the only remaining Rc owner is `contacts`. Then
    // `try_unwrap` cannot fail.
    drop(block);
    Ok(Rc::try_unwrap(contacts)
        .expect("block was dropped above; Rc should be uniquely owned")
        .into_inner())
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
            if nserror_is_record_not_found(&err) {
                Ok(None)
            } else {
                Err(ContactsError::ObjcError(
                    err.localizedDescription().to_string(),
                ))
            }
        }
    }
}

fn parse_contact_type(s: &str) -> Result<CNContactType, ContactsError> {
    match s {
        "person" => Ok(CNContactType::Person),
        "organization" => Ok(CNContactType::Organization),
        other => Err(ContactsError::InvalidInput(format!(
            "contact_type must be 'person' or 'organization', got '{other}'"
        ))),
    }
}

/// Parse a birthday string into an NSDateComponents.
///
/// Accepts `YYYY-MM-DD` (four-digit year followed by two-digit month and day,
/// separated by hyphens) or `--MM-DD` (month/day only, no year). All components
/// must be numeric and within their normal ranges (months 1..=12, days 1..=31).
fn parse_birthday(s: &str) -> Result<Retained<NSDateComponents>, ContactsError> {
    fn bad(s: &str) -> ContactsError {
        ContactsError::InvalidInput(format!(
            "birthday must be 'YYYY-MM-DD' or '--MM-DD', got '{s}'"
        ))
    }

    let (year, month, day) = if let Some(rest) = s.strip_prefix("--") {
        let mut parts = rest.split('-');
        let m = parts.next().ok_or_else(|| bad(s))?;
        let d = parts.next().ok_or_else(|| bad(s))?;
        if parts.next().is_some() {
            return Err(bad(s));
        }
        (None, m, d)
    } else {
        let mut parts = s.split('-');
        let y = parts.next().ok_or_else(|| bad(s))?;
        let m = parts.next().ok_or_else(|| bad(s))?;
        let d = parts.next().ok_or_else(|| bad(s))?;
        if parts.next().is_some() {
            return Err(bad(s));
        }
        (Some(y), m, d)
    };

    let month: i64 = month.parse().map_err(|_| bad(s))?;
    let day: i64 = day.parse().map_err(|_| bad(s))?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(bad(s));
    }
    let year: Option<i64> = year
        .map(|y| y.parse::<i64>().map_err(|_| bad(s)))
        .transpose()?;

    let comps = NSDateComponents::new();
    if let Some(y) = year {
        comps.setYear(y as isize);
    } else {
        comps.setYear(NS_DATE_COMPONENT_UNDEFINED);
    }
    comps.setMonth(month as isize);
    comps.setDay(day as isize);
    Ok(comps)
}

/// Build a CNMutablePostalAddress from our structured input. Missing sub-fields
/// remain empty NSStrings (same as Contacts.app when the user leaves them blank).
///
/// # Safety
///
/// Constructs a fresh CNMutablePostalAddress and calls its setters. Setters on a
/// freshly allocated object are always sound.
unsafe fn build_postal_address(data: &PostalAddressData) -> Retained<CNMutablePostalAddress> {
    unsafe {
        let addr = CNMutablePostalAddress::new();
        if let Some(ref v) = data.street {
            addr.setStreet(&NSString::from_str(v));
        }
        if let Some(ref v) = data.city {
            addr.setCity(&NSString::from_str(v));
        }
        if let Some(ref v) = data.state {
            addr.setState(&NSString::from_str(v));
        }
        if let Some(ref v) = data.postal_code {
            addr.setPostalCode(&NSString::from_str(v));
        }
        if let Some(ref v) = data.country {
            addr.setCountry(&NSString::from_str(v));
        }
        addr
    }
}

/// All scalar fields shared between create and update, borrowed from the
/// caller's params struct. Keeping these together gives `apply_scalar_fields`
/// one logical argument instead of 16 positional ones.
struct ScalarFields<'a> {
    contact_type: Option<&'a String>,
    name_prefix: Option<&'a String>,
    first_name: Option<&'a String>,
    middle_name: Option<&'a String>,
    last_name: Option<&'a String>,
    name_suffix: Option<&'a String>,
    nickname: Option<&'a String>,
    phonetic_given_name: Option<&'a String>,
    phonetic_middle_name: Option<&'a String>,
    phonetic_family_name: Option<&'a String>,
    phonetic_organization_name: Option<&'a String>,
    organization: Option<&'a String>,
    department: Option<&'a String>,
    job_title: Option<&'a String>,
    note: Option<&'a String>,
    birthday: Option<&'a String>,
}

impl<'a> ScalarFields<'a> {
    fn from_create(p: &'a ContactsCreateParams) -> Self {
        Self {
            contact_type: p.contact_type.as_ref(),
            name_prefix: p.name_prefix.as_ref(),
            first_name: p.first_name.as_ref(),
            middle_name: p.middle_name.as_ref(),
            last_name: p.last_name.as_ref(),
            name_suffix: p.name_suffix.as_ref(),
            nickname: p.nickname.as_ref(),
            phonetic_given_name: p.phonetic_given_name.as_ref(),
            phonetic_middle_name: p.phonetic_middle_name.as_ref(),
            phonetic_family_name: p.phonetic_family_name.as_ref(),
            phonetic_organization_name: p.phonetic_organization_name.as_ref(),
            organization: p.organization.as_ref(),
            department: p.department.as_ref(),
            job_title: p.job_title.as_ref(),
            note: p.note.as_ref(),
            birthday: p.birthday.as_ref(),
        }
    }

    fn from_update(p: &'a ContactsUpdateParams) -> Self {
        Self {
            contact_type: p.contact_type.as_ref(),
            name_prefix: p.name_prefix.as_ref(),
            first_name: p.first_name.as_ref(),
            middle_name: p.middle_name.as_ref(),
            last_name: p.last_name.as_ref(),
            name_suffix: p.name_suffix.as_ref(),
            nickname: p.nickname.as_ref(),
            phonetic_given_name: p.phonetic_given_name.as_ref(),
            phonetic_middle_name: p.phonetic_middle_name.as_ref(),
            phonetic_family_name: p.phonetic_family_name.as_ref(),
            phonetic_organization_name: p.phonetic_organization_name.as_ref(),
            organization: p.organization.as_ref(),
            department: p.department.as_ref(),
            job_title: p.job_title.as_ref(),
            note: p.note.as_ref(),
            birthday: p.birthday.as_ref(),
        }
    }
}

/// Set every non-None scalar on the contact. Shared by create and update.
///
/// # Safety
///
/// Caller must hold a live CNMutableContact. All setters are sound on a
/// just-constructed or just-copied mutable contact.
unsafe fn apply_scalar_fields(
    mutable: &CNMutableContact,
    fields: &ScalarFields<'_>,
) -> Result<(), ContactsError> {
    unsafe {
        if let Some(v) = fields.contact_type {
            mutable.setContactType(parse_contact_type(v)?);
        }
        if let Some(v) = fields.name_prefix {
            mutable.setNamePrefix(&NSString::from_str(v));
        }
        if let Some(v) = fields.first_name {
            mutable.setGivenName(&NSString::from_str(v));
        }
        if let Some(v) = fields.middle_name {
            mutable.setMiddleName(&NSString::from_str(v));
        }
        if let Some(v) = fields.last_name {
            mutable.setFamilyName(&NSString::from_str(v));
        }
        if let Some(v) = fields.name_suffix {
            mutable.setNameSuffix(&NSString::from_str(v));
        }
        if let Some(v) = fields.nickname {
            mutable.setNickname(&NSString::from_str(v));
        }
        if let Some(v) = fields.phonetic_given_name {
            mutable.setPhoneticGivenName(&NSString::from_str(v));
        }
        if let Some(v) = fields.phonetic_middle_name {
            mutable.setPhoneticMiddleName(&NSString::from_str(v));
        }
        if let Some(v) = fields.phonetic_family_name {
            mutable.setPhoneticFamilyName(&NSString::from_str(v));
        }
        if let Some(v) = fields.phonetic_organization_name {
            mutable.setPhoneticOrganizationName(&NSString::from_str(v));
        }
        if let Some(v) = fields.organization {
            mutable.setOrganizationName(&NSString::from_str(v));
        }
        if let Some(v) = fields.department {
            mutable.setDepartmentName(&NSString::from_str(v));
        }
        if let Some(v) = fields.job_title {
            mutable.setJobTitle(&NSString::from_str(v));
        }
        if let Some(v) = fields.note {
            mutable.setNote(&NSString::from_str(v));
        }
        if let Some(v) = fields.birthday {
            let comps = parse_birthday(v)?;
            mutable.setBirthday(Some(&comps));
        }
    }
    Ok(())
}

fn create_contact_impl(
    store: &CNContactStore,
    params: &ContactsCreateParams,
) -> Result<String, ContactsError> {
    unsafe {
        let contact = CNMutableContact::new();

        apply_scalar_fields(&contact, &ScalarFields::from_create(params))?;

        if let Some(ref email) = params.email {
            let email_value = NSString::from_str(email);
            let label = NSString::from_str("work");
            let labeled = CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*email_value);
            contact.setEmailAddresses(&NSArray::from_retained_slice(&[labeled]));
        }

        if let Some(phone) = params.phone.as_deref()
            && let Some(phone_number) =
                CNPhoneNumber::phoneNumberWithStringValue(&NSString::from_str(phone))
        {
            let label = NSString::from_str("mobile");
            let labeled = CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*phone_number);
            contact.setPhoneNumbers(&NSArray::from_retained_slice(&[labeled]));
        }

        if let Some(ref url) = params.url {
            let url_value = NSString::from_str(url);
            let label = NSString::from_str("homepage");
            let labeled = CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*url_value);
            contact.setUrlAddresses(&NSArray::from_retained_slice(&[labeled]));
        }

        if let Some(ref data) = params.postal_address
            && !data.is_empty()
        {
            let addr = build_postal_address(data);
            let label = NSString::from_str("home");
            let labeled: Retained<CNLabeledValue<CNPostalAddress>> =
                CNLabeledValue::labeledValueWithLabel_value(Some(&label), &**addr);
            contact.setPostalAddresses(&NSArray::from_retained_slice(&[labeled]));
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
        .map_err(|err| {
            if nserror_is_record_not_found(&err) {
                ContactsError::ContactNotFound(params.id.clone())
            } else {
                ContactsError::ObjcError(err.localizedDescription().to_string())
            }
        })?;

    unsafe {
        let mutable: Retained<CNMutableContact> = contact.mutableCopy();

        apply_scalar_fields(&mutable, &ScalarFields::from_update(params))?;

        if let Some(ref op) = params.phone_op {
            apply_phone_op(&mutable, op, &params.id)?;
        }
        if let Some(ref op) = params.email_op {
            apply_email_op(&mutable, op, &params.id)?;
        }
        if let Some(ref op) = params.url_op {
            apply_url_op(&mutable, op, &params.id)?;
        }
        if let Some(ref op) = params.postal_op {
            apply_postal_op(&mutable, op, &params.id)?;
        }

        let save_request = CNSaveRequest::new();
        save_request.updateContact(&mutable);

        store
            .executeSaveRequest_error(&save_request)
            .map_err(|err| ContactsError::ObjcError(err.localizedDescription().to_string()))?;
    }

    Ok(true)
}

/// Apply a phone add/remove/replace op to a mutable contact.
///
/// # Safety
///
/// Caller must ensure `mutable` is a valid CNMutableContact with phoneNumbers
/// fetched (any contact loaded with `contact_fetch_keys_array` qualifies).
unsafe fn apply_phone_op(
    mutable: &CNMutableContact,
    op: &PhoneOp,
    contact_id: &str,
) -> Result<(), ContactsError> {
    unsafe {
        let existing = mutable.phoneNumbers();
        let count = existing.len();

        match op {
            PhoneOp::Add(to) => {
                let new_number = CNPhoneNumber::phoneNumberWithStringValue(&NSString::from_str(to))
                    .ok_or_else(|| {
                        ContactsError::InvalidInput(format!("invalid phone number: {to}"))
                    })?;
                let label = NSString::from_str("mobile");
                let new_labeled =
                    CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*new_number);
                let mut entries: Vec<Retained<CNLabeledValue<CNPhoneNumber>>> =
                    (0..count).map(|i| existing.objectAtIndex(i)).collect();
                entries.push(new_labeled);
                mutable.setPhoneNumbers(&NSArray::from_retained_slice(&entries));
            }
            PhoneOp::Remove(from) => {
                let mut entries: Vec<Retained<CNLabeledValue<CNPhoneNumber>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().stringValue().to_string();
                    if !matched && current == *from {
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "phone '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setPhoneNumbers(&NSArray::from_retained_slice(&entries));
            }
            PhoneOp::Replace { from, to } => {
                let new_number = CNPhoneNumber::phoneNumberWithStringValue(&NSString::from_str(to))
                    .ok_or_else(|| {
                        ContactsError::InvalidInput(format!("invalid phone number: {to}"))
                    })?;
                let mut entries: Vec<Retained<CNLabeledValue<CNPhoneNumber>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().stringValue().to_string();
                    if !matched && current == *from {
                        entries.push(labeled.labeledValueBySettingValue(&*new_number));
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "phone '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setPhoneNumbers(&NSArray::from_retained_slice(&entries));
            }
        }
    }
    Ok(())
}

/// Apply an email add/remove/replace op to a mutable contact.
///
/// # Safety
///
/// Caller must ensure `mutable` is a valid CNMutableContact with emailAddresses
/// fetched (any contact loaded with `contact_fetch_keys_array` qualifies).
unsafe fn apply_email_op(
    mutable: &CNMutableContact,
    op: &EmailOp,
    contact_id: &str,
) -> Result<(), ContactsError> {
    unsafe {
        let existing = mutable.emailAddresses();
        let count = existing.len();

        match op {
            EmailOp::Add(to) => {
                let new_value = NSString::from_str(to);
                let label = NSString::from_str("work");
                let new_labeled =
                    CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*new_value);
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    (0..count).map(|i| existing.objectAtIndex(i)).collect();
                entries.push(new_labeled);
                mutable.setEmailAddresses(&NSArray::from_retained_slice(&entries));
            }
            EmailOp::Remove(from) => {
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().to_string();
                    if !matched && current == *from {
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "email '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setEmailAddresses(&NSArray::from_retained_slice(&entries));
            }
            EmailOp::Replace { from, to } => {
                let new_value = NSString::from_str(to);
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().to_string();
                    if !matched && current == *from {
                        entries.push(labeled.labeledValueBySettingValue(&*new_value));
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "email '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setEmailAddresses(&NSArray::from_retained_slice(&entries));
            }
        }
    }
    Ok(())
}

/// Apply a URL add/remove/replace op to a mutable contact.
///
/// # Safety
///
/// Caller must ensure `mutable` is a valid CNMutableContact with urlAddresses
/// fetched.
unsafe fn apply_url_op(
    mutable: &CNMutableContact,
    op: &UrlOp,
    contact_id: &str,
) -> Result<(), ContactsError> {
    unsafe {
        let existing = mutable.urlAddresses();
        let count = existing.len();

        match op {
            UrlOp::Add(to) => {
                let new_value = NSString::from_str(to);
                let label = NSString::from_str("homepage");
                let new_labeled =
                    CNLabeledValue::labeledValueWithLabel_value(Some(&label), &*new_value);
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    (0..count).map(|i| existing.objectAtIndex(i)).collect();
                entries.push(new_labeled);
                mutable.setUrlAddresses(&NSArray::from_retained_slice(&entries));
            }
            UrlOp::Remove(from) => {
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().to_string();
                    if !matched && current == *from {
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "url '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setUrlAddresses(&NSArray::from_retained_slice(&entries));
            }
            UrlOp::Replace { from, to } => {
                let new_value = NSString::from_str(to);
                let mut entries: Vec<Retained<CNLabeledValue<NSString>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = labeled.value().to_string();
                    if !matched && current == *from {
                        entries.push(labeled.labeledValueBySettingValue(&*new_value));
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "url '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setUrlAddresses(&NSArray::from_retained_slice(&entries));
            }
        }
    }
    Ok(())
}

/// Apply a postal-address add/remove/replace op to a mutable contact.
///
/// Existing entries are matched by their joined-string form, which is the same
/// representation exposed in `Contact.addresses`. On replace, the original
/// label is preserved.
///
/// # Safety
///
/// Caller must ensure `mutable` is a valid CNMutableContact with postalAddresses
/// fetched.
unsafe fn apply_postal_op(
    mutable: &CNMutableContact,
    op: &PostalOp,
    contact_id: &str,
) -> Result<(), ContactsError> {
    unsafe {
        let existing = mutable.postalAddresses();
        let count = existing.len();

        match op {
            PostalOp::Add(data) => {
                if data.is_empty() {
                    return Err(ContactsError::InvalidInput(
                        "postal address requires at least one non-empty field".into(),
                    ));
                }
                let addr = build_postal_address(data);
                let label = NSString::from_str("home");
                let new_labeled: Retained<CNLabeledValue<CNPostalAddress>> =
                    CNLabeledValue::labeledValueWithLabel_value(Some(&label), &**addr);
                let mut entries: Vec<Retained<CNLabeledValue<CNPostalAddress>>> =
                    (0..count).map(|i| existing.objectAtIndex(i)).collect();
                entries.push(new_labeled);
                mutable.setPostalAddresses(&NSArray::from_retained_slice(&entries));
            }
            PostalOp::Remove(from) => {
                let mut entries: Vec<Retained<CNLabeledValue<CNPostalAddress>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = format_postal_address(&labeled.value());
                    if !matched && current == *from {
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "postal address '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setPostalAddresses(&NSArray::from_retained_slice(&entries));
            }
            PostalOp::Replace { from, to } => {
                if to.is_empty() {
                    return Err(ContactsError::InvalidInput(
                        "postal address requires at least one non-empty field".into(),
                    ));
                }
                let new_addr = build_postal_address(to);
                let mut entries: Vec<Retained<CNLabeledValue<CNPostalAddress>>> =
                    Vec::with_capacity(count);
                let mut matched = false;
                for i in 0..count {
                    let labeled = existing.objectAtIndex(i);
                    let current = format_postal_address(&labeled.value());
                    if !matched && current == *from {
                        entries.push(labeled.labeledValueBySettingValue(&**new_addr));
                        matched = true;
                    } else {
                        entries.push(labeled);
                    }
                }
                if !matched {
                    return Err(ContactsError::InvalidInput(format!(
                        "postal address '{from}' not found on contact {contact_id}",
                    )));
                }
                mutable.setPostalAddresses(&NSArray::from_retained_slice(&entries));
            }
        }
    }
    Ok(())
}

fn delete_contact_impl(store: &CNContactStore, id: &str) -> Result<bool, ContactsError> {
    let keys = contact_fetch_keys_array();
    let ns_id = NSString::from_str(id);

    let contact = unsafe { store.unifiedContactWithIdentifier_keysToFetch_error(&ns_id, &keys) }
        .map_err(|err| {
            if nserror_is_record_not_found(&err) {
                ContactsError::ContactNotFound(id.to_string())
            } else {
                ContactsError::ObjcError(err.localizedDescription().to_string())
            }
        })?;

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
        // SAFETY: request and block live through the call; error is an out-param.
        let success = unsafe {
            store.enumerateContactsWithFetchRequest_error_usingBlock(
                &request,
                Some(&mut error),
                &block,
            )
        };
        if !success {
            let detail = error
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "unknown error".to_string());
            tracing::warn!(
                group = %group_name,
                error = %detail,
                "failed to enumerate members while counting group"
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

    let group_id = (0..groups.len())
        .find_map(|i| {
            let group = groups.objectAtIndex(i);
            // SAFETY: `group` is a live CNGroup returned by the matching-predicate query.
            unsafe { group.name() }
                .to_string()
                .eq_ignore_ascii_case(name)
                .then(|| unsafe { group.identifier() }.to_string())
        })
        .ok_or_else(|| ContactsError::GroupNotFound(name.into()))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_contact_type_accepts_person_and_organization() {
        assert_eq!(parse_contact_type("person").unwrap(), CNContactType::Person);
        assert_eq!(
            parse_contact_type("organization").unwrap(),
            CNContactType::Organization,
        );
    }

    #[test]
    fn parse_contact_type_rejects_other_values() {
        let err = parse_contact_type("Person").unwrap_err();
        assert!(err.to_string().contains("'person' or 'organization'"));
        assert!(parse_contact_type("company").is_err());
        assert!(parse_contact_type("").is_err());
    }

    #[test]
    fn parse_birthday_accepts_full_date() {
        let comps = parse_birthday("1815-12-10").unwrap();
        assert_eq!(comps.year(), 1815);
        assert_eq!(comps.month(), 12);
        assert_eq!(comps.day(), 10);
    }

    #[test]
    fn parse_birthday_accepts_no_year() {
        let comps = parse_birthday("--06-15").unwrap();
        assert_eq!(comps.year(), NS_DATE_COMPONENT_UNDEFINED);
        assert_eq!(comps.month(), 6);
        assert_eq!(comps.day(), 15);
    }

    #[test]
    fn parse_birthday_rejects_malformed_inputs() {
        for bad in [
            "",
            "1815/12/10",
            "1815-12",
            "1815-12-10-extra",
            "abc-12-10",
            "1815-13-10",
            "1815-12-32",
            "1815-00-10",
            "1815-12-00",
            "-12-10",
            "--13-10",
        ] {
            assert!(
                parse_birthday(bad).is_err(),
                "expected parse_birthday({bad:?}) to fail",
            );
        }
    }

    #[test]
    fn none_if_empty_converts_empty_to_none() {
        assert_eq!(none_if_empty(String::new()), None);
        assert_eq!(none_if_empty("x".into()), Some("x".into()));
    }

    #[test]
    fn postal_address_data_is_empty_detects_all_none() {
        let empty = PostalAddressData::default();
        assert!(empty.is_empty());
        let populated = PostalAddressData {
            street: Some("123 Main".into()),
            ..Default::default()
        };
        assert!(!populated.is_empty());
    }
}
