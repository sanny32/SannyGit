//! Hosting-specific clone source forms. Authentication is delegated to Git.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Provider {
    #[default]
    Url,
    GitHub,
    GitHubEnterprise,
    Bitbucket,
    BitbucketServer,
    GitLab,
    GitLabSelfHosted,
    Azure,
    AzureServer,
}

impl Provider {
    pub const ALL: [Self; 9] = [
        Self::Url,
        Self::GitHub,
        Self::GitHubEnterprise,
        Self::Bitbucket,
        Self::BitbucketServer,
        Self::GitLab,
        Self::GitLabSelfHosted,
        Self::Azure,
        Self::AzureServer,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Url => "URL or Path",
            Self::GitHub => "GitHub",
            Self::GitHubEnterprise => "GitHub Enterprise",
            Self::Bitbucket => "Bitbucket",
            Self::BitbucketServer => "Bitbucket Server",
            Self::GitLab => "GitLab",
            Self::GitLabSelfHosted => "GitLab (Self Hosted)",
            Self::Azure => "Azure DevOps",
            Self::AzureServer => "Azure DevOps Server",
        }
    }

    pub fn self_hosted(self) -> bool {
        matches!(
            self,
            Self::GitHubEnterprise
                | Self::BitbucketServer
                | Self::GitLabSelfHosted
                | Self::AzureServer
        )
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Url => "https://server/path/repository.git or a local path",
            Self::GitHub | Self::GitHubEnterprise => "owner/repository",
            Self::Bitbucket => "workspace/repository",
            Self::BitbucketServer => "project/repository",
            Self::GitLab | Self::GitLabSelfHosted => "group/subgroup/repository",
            Self::Azure => "organization/project/repository",
            Self::AzureServer => "project/repository (server URL includes collection)",
        }
    }
}

#[derive(Default)]
pub struct Form {
    pub provider: Provider,
    inputs: [String; 9],
    servers: [String; 9],
}

impl Form {
    pub fn input(&mut self) -> &mut String {
        &mut self.inputs[self.provider as usize]
    }

    pub fn server(&mut self) -> &mut String {
        &mut self.servers[self.provider as usize]
    }

    pub fn resolve(&self) -> Result<String, String> {
        let input = self.inputs[self.provider as usize].trim();
        if input.is_empty() {
            return Err("Enter a repository URL or path.".into());
        }
        if self.provider == Provider::Url || input.contains("://") || input.starts_with("git@") {
            return Ok(input.into());
        }
        let parts: Vec<_> = input.split('/').collect();
        let count = match self.provider {
            Provider::Azure => parts.len() == 3,
            Provider::GitLab | Provider::GitLabSelfHosted => parts.len() >= 2,
            _ => parts.len() == 2,
        };
        if !count
            || parts
                .iter()
                .any(|p| p.is_empty() || matches!(*p, "." | ".."))
            || input.chars().any(|c| {
                c.is_control()
                    || c.is_whitespace()
                    || matches!(c, '?' | '#' | '@' | ':' | '\\' | '%')
            })
        {
            return Err(format!(
                "Enter {} or paste a full clone URL.",
                self.provider.hint()
            ));
        }
        let base = match self.provider {
            Provider::GitHub => "https://github.com".to_owned(),
            Provider::Bitbucket => "https://bitbucket.org".to_owned(),
            Provider::GitLab => "https://gitlab.com".to_owned(),
            Provider::Azure => "https://dev.azure.com".to_owned(),
            _ => {
                let server = self.servers[self.provider as usize]
                    .trim()
                    .trim_end_matches('/');
                let url =
                    reqwest::Url::parse(server).map_err(|_| "Enter a full HTTP(S) server URL.")?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return Err(
                        "Enter an HTTP(S) server URL without credentials, query, or fragment."
                            .into(),
                    );
                }
                server.to_owned()
            }
        };
        Ok(match self.provider {
            Provider::Azure => format!("{base}/{}/{}/_git/{}", parts[0], parts[1], parts[2]),
            Provider::AzureServer => format!("{base}/{}/_git/{}", parts[0], parts[1]),
            Provider::BitbucketServer => {
                format!("{base}/scm/{}.git", input.trim_end_matches(".git"))
            }
            _ => format!("{base}/{}.git", input.trim_end_matches(".git")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_every_provider_and_keeps_each_form() {
        let mut form = Form::default();
        for (provider, input, server, expected) in [
            (Provider::Url, " ~/repo ", "", "~/repo"),
            (
                Provider::GitHub,
                "owner/repo",
                "",
                "https://github.com/owner/repo.git",
            ),
            (
                Provider::GitHubEnterprise,
                "owner/repo.git",
                "https://github.example/",
                "https://github.example/owner/repo.git",
            ),
            (
                Provider::Bitbucket,
                "team/repo",
                "",
                "https://bitbucket.org/team/repo.git",
            ),
            (
                Provider::BitbucketServer,
                "PROJ/repo",
                "https://bitbucket.example:8443/context",
                "https://bitbucket.example:8443/context/scm/PROJ/repo.git",
            ),
            (
                Provider::GitLab,
                "group/subgroup/repo",
                "",
                "https://gitlab.com/group/subgroup/repo.git",
            ),
            (
                Provider::GitLabSelfHosted,
                "group/repo",
                "https://gitlab.example",
                "https://gitlab.example/group/repo.git",
            ),
            (
                Provider::Azure,
                "org/project/repo",
                "",
                "https://dev.azure.com/org/project/_git/repo",
            ),
            (
                Provider::AzureServer,
                "project/repo",
                "http://azure.example/tfs/collection",
                "http://azure.example/tfs/collection/project/_git/repo",
            ),
        ] {
            form.provider = provider;
            *form.input() = input.into();
            *form.server() = server.into();
            assert_eq!(form.resolve().unwrap(), expected);
        }
        form.provider = Provider::GitHub;
        assert_eq!(form.input(), "owner/repo");
        for provider in Provider::ALL {
            form.provider = provider;
            *form.input() = "ssh://git@host:2222/context/repo.git".into();
            assert_eq!(
                form.resolve().unwrap(),
                "ssh://git@host:2222/context/repo.git"
            );
        }
    }

    #[test]
    fn rejects_incomplete_paths_and_invalid_server_addresses() {
        let mut form = Form {
            provider: Provider::GitHub,
            ..Default::default()
        };
        for input in [
            "",
            "repo",
            "owner/",
            "../repo",
            "owner/repo?x",
            "owner/re\npo",
            "owner//repo",
            "owner/repo name",
        ] {
            *form.input() = input.into();
            assert!(form.resolve().is_err(), "{input:?}");
        }
        form.provider = Provider::GitLabSelfHosted;
        *form.input() = "group/repo".into();
        for server in [
            "",
            "host",
            "file:///tmp",
            "https://user:secret@host",
            "https://host?query",
            "https://host#fragment",
        ] {
            *form.server() = server.into();
            assert!(form.resolve().is_err(), "{server:?}");
        }
    }
}
