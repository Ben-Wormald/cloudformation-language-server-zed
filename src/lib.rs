use std::{env, fs};
use zed_extension_api::{
    self as zed, Command, Extension, GithubRelease, GithubReleaseOptions, LanguageServerId,
    LanguageServerInstallationStatus, Result, Worktree, latest_github_release, node_binary_path, register_extension,
    serde_json::Value, set_language_server_installation_status, settings::LspSettings,
};

const SERVER_REPO: &str = "aws-cloudformation/cloudformation-languageserver";
const SERVER_SCRIPT: &str = "cfn-lsp-server-standalone.js";
const EXTENSION_LS_NAME: &str = "cloudformation-language-server";

struct CloudFormationLanguageServer {
    server_path: Option<String>,
}

impl CloudFormationLanguageServer {
    fn get_server_path(&mut self, language_server_id: &LanguageServerId, worktree: &Worktree) -> zed::Result<String> {
        if let Some(server_path) = &self.server_path {
            Ok(server_path.clone())
        } else {
            Self::get_server_installation(language_server_id, worktree).inspect_err(|error| {
                set_language_server_installation_status(
                    language_server_id,
                    &LanguageServerInstallationStatus::Failed(error.clone()),
                );
            })
        }
    }

    fn get_server_installation(language_server_id: &LanguageServerId, worktree: &Worktree) -> zed::Result<String> {
        if let Ok(settings) = LspSettings::for_worktree(EXTENSION_LS_NAME, worktree)
            && let Some(binary) = settings.binary
            && let Some(path) = binary.path
        {
            return Ok(path);
        }

        set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = latest_github_release(
            SERVER_REPO,
            GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let cwd = env::current_dir()
            .or(zed::Result::Err("Failed to get current working directory"))?
            .to_str()
            .ok_or(())
            .or(zed::Result::Err("Failed to get current working directory"))?
            .to_owned();

        let version_dir = format!("cloudformation-languageserver-{version}", version = release.version);

        let server_dir = format!("{cwd}/{version_dir}",);

        if fs::metadata(&server_dir).is_err() {
            set_language_server_installation_status(language_server_id, &LanguageServerInstallationStatus::Downloading);
            Self::download_server(&server_dir, release)?;

            if let Ok(entries) = fs::read_dir(cwd) {
                for entry in entries.flatten() {
                    if let Ok(name) = entry.file_name().into_string()
                        && name.starts_with("cloudformation-languageserver-")
                        && !server_dir.ends_with(&name)
                    {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }

        set_language_server_installation_status(language_server_id, &LanguageServerInstallationStatus::None);

        let server_path = format!("{server_dir}/{SERVER_SCRIPT}");
        Ok(server_path)
    }

    fn download_server(server_dir: &str, release: GithubRelease) -> zed::Result<()> {
        let (platform, arch) = zed::current_platform();

        let build = format!(
            "{os}-{arch}",
            os = match platform {
                zed::Os::Mac => "darwin",
                zed::Os::Linux => "linux",
                zed::Os::Windows => "win32",
            },
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X8664 => "x64",
                zed::Architecture::X86 => {
                    return Result::Err(String::from("Unsupported architecture"));
                },
            },
        );

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name.contains(&build))
            .ok_or(String::from("release not found"))?;

        zed::download_file(&asset.download_url, server_dir, zed::DownloadedFileType::Zip)
    }
}

impl Extension for CloudFormationLanguageServer {
    fn new() -> Self {
        Self { server_path: None }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command> {
        let server_path = self.get_server_path(language_server_id, worktree)?;

        Ok(Command {
            command: node_binary_path()?,
            args: vec![server_path, String::from("--stdio")],
            env: Vec::default(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let lsp_settings = LspSettings::for_worktree(EXTENSION_LS_NAME, worktree)?;
        Ok(lsp_settings.initialization_options)
    }

    fn language_server_workspace_configuration(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<Value>> {
        let lsp_settings = LspSettings::for_worktree(EXTENSION_LS_NAME, worktree)?;
        Ok(lsp_settings.settings)
    }
}

register_extension!(CloudFormationLanguageServer);
