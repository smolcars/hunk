use crate::models::{ForgeProvider, ForgeRepoRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeCredentialMetadata {
    pub id: String,
    pub provider: ForgeProvider,
    pub host: String,
    pub account_label: String,
    pub is_default_for_host: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeRepoCredentialBinding {
    pub provider: ForgeProvider,
    pub host: String,
    pub repo_path: String,
    pub credential_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeCredentialResolution {
    RepoBinding,
    HostDefault,
    SingleHostCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedForgeCredential {
    pub credential_id: String,
    pub resolution: ForgeCredentialResolution,
}

pub fn resolve_credential_for_repo(
    repo: &ForgeRepoRef,
    credentials: &[ForgeCredentialMetadata],
    bindings: &[ForgeRepoCredentialBinding],
) -> Option<ResolvedForgeCredential> {
    let host_credentials = credentials
        .iter()
        .filter(|credential| credential.provider == repo.provider && credential.host == repo.host)
        .collect::<Vec<_>>();

    if host_credentials.is_empty() {
        return None;
    }

    if let Some(binding) = bindings.iter().find(|binding| {
        binding.provider == repo.provider
            && binding.host == repo.host
            && binding.repo_path == repo.path
    }) && host_credentials
        .iter()
        .any(|credential| credential.id == binding.credential_id)
    {
        return Some(ResolvedForgeCredential {
            credential_id: binding.credential_id.clone(),
            resolution: ForgeCredentialResolution::RepoBinding,
        });
    }

    let host_defaults = host_credentials
        .iter()
        .filter(|credential| credential.is_default_for_host)
        .collect::<Vec<_>>();
    if host_defaults.len() == 1 {
        return Some(ResolvedForgeCredential {
            credential_id: host_defaults[0].id.clone(),
            resolution: ForgeCredentialResolution::HostDefault,
        });
    }

    if host_credentials.len() == 1 {
        return Some(ResolvedForgeCredential {
            credential_id: host_credentials[0].id.clone(),
            resolution: ForgeCredentialResolution::SingleHostCredential,
        });
    }

    None
}
