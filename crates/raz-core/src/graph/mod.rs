//! Microsoft Graph access for directory operations (`raz ad ...`). [`client::GraphClient`] is
//! the REST transport; [`federated_credential`] manages an app registration's federated
//! identity credentials, mirroring `az ad app federated-credential`.

pub mod client;
pub mod federated_credential;
