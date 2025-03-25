/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

//! `EdenFsInstance` provides access to configuration, socket paths, client directories,
//! and other daemon-related (EdenFS) resources. It is designed to be initialized once
//! and accessed globally throughout your application.
//!
//! # Examples
//!
//! ## Initializing the global instance
//!
//! ```
//! use edenfs_client::instance::EdenFsInstance;
//! use edenfs_client::utils::get_config_dir;
//! use edenfs_client::utils::get_etc_eden_dir;
//! use edenfs_client::utils::get_home_dir;
//!
//! // Initialize the global instance
//! EdenFsInstance::init(
//!     get_config_dir(&None, &None).unwrap(),
//!     get_etc_eden_dir(&None),
//!     get_home_dir(&None),
//! );
//!
//! // Access the global instance
//! let instance = EdenFsInstance::global();
//! ```
//!
//! ## Getting EdenFS configuration
//!
//! ```
//! use std::path::PathBuf;
//!
//! use edenfs_client::instance::EdenFsInstance;
//! use edenfs_client::utils::get_config_dir;
//! use edenfs_client::utils::get_etc_eden_dir;
//! use edenfs_client::utils::get_home_dir;
//!
//! EdenFsInstance::init(
//!     get_config_dir(&None, &None).unwrap(),
//!     get_etc_eden_dir(&None),
//!     get_home_dir(&None),
//! );
//! let instance = EdenFsInstance::global();
//! match instance.get_config() {
//!     Ok(config) => {
//!         println!("Successfully loaded EdenFS configuration");
//!         // Use config...
//!     }
//!     Err(err) => {
//!         eprintln!("Failed to load EdenFS configuration: {}", err);
//!     }
//! }
//! ```
//!
//! ## Working with mounts
//!
//! ```
//! use std::path::Path;
//!
//! use edenfs_client::instance::EdenFsInstance;
//! use edenfs_client::utils::get_config_dir;
//! use edenfs_client::utils::get_etc_eden_dir;
//! use edenfs_client::utils::get_home_dir;
//!
//! EdenFsInstance::init(
//!     get_config_dir(&None, &None).unwrap(),
//!     get_etc_eden_dir(&None),
//!     get_home_dir(&None),
//! );
//! let instance = EdenFsInstance::global();
//! match instance.get_configured_mounts_map() {
//!     Ok(mounts) => {
//!         println!("Configured mounts:");
//!         for (path, name) in mounts {
//!             println!("  {} -> {}", path.display(), name);
//!         }
//!     }
//!     Err(err) => {
//!         eprintln!("Failed to get configured mounts: {}", err);
//!     }
//! }
//! ```

use std::collections::BTreeMap;
#[cfg(windows)]
use std::fs::remove_file;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::anyhow;
use anyhow::Context;
use atomicfile::atomic_write;
use edenfs_config::EdenFsConfig;
use edenfs_error::EdenFsError;
use edenfs_error::Result;
use edenfs_error::ResultExt;
use edenfs_utils::get_executable;
#[cfg(windows)]
use edenfs_utils::strip_unc_prefix;
use fbinit::expect_init;
use tracing::event;
use tracing::Level;
use util::lock::PathLock;

use crate::client::EdenFsClient;
use crate::client::StreamingEdenFsClient;

// We create a single EdenFsInstance when starting up and utilize EdenFsInstance::global()
// whenever we need to access it.
static INSTANCE: OnceLock<EdenFsInstance> = OnceLock::new();

// Default config and etc dirs
#[cfg(unix)]
pub const DEFAULT_CONFIG_DIR: &str = "~/local/.eden";
#[cfg(unix)]
pub const DEFAULT_ETC_EDEN_DIR: &str = "/etc/eden";

#[cfg(windows)]
pub const DEFAULT_CONFIG_DIR: &str = "~\\.eden";
#[cfg(windows)]
pub const DEFAULT_ETC_EDEN_DIR: &str = "C:\\ProgramData\\facebook\\eden";

/// These paths are relative to the user's client directory.
const CLIENTS_DIR: &str = "clients";
const CONFIG_JSON: &str = "config.json";
const CONFIG_JSON_LOCK: &str = "config.json.lock";
const CONFIG_JSON_MODE: u32 = 0o664;

/// Manages daemon-related (EdenFS) resources besides the Thrift connection.
///
/// `EdenFsInstance` provides access to configuration, socket paths, client directories,
/// and other daemon-related (EdenFS) resources. It is designed to be initialized once and accessed
/// globally throughout your application.
///
/// # Fields
///
/// * `config_dir` - Path to the EdenFS configuration directory
/// * `etc_eden_dir` - Path to the system-wide EdenFS configuration directory
/// * `home_dir` - Optional path to the user's home directory
#[derive(Debug)]
pub struct EdenFsInstance {
    config_dir: PathBuf,
    etc_eden_dir: PathBuf,
    home_dir: Option<PathBuf>,
}

impl EdenFsInstance {
    /// Returns a reference to the global `EdenFsInstance`.
    ///
    /// This method provides access to the global instance that was previously initialized
    /// with [`init`](Self::init). It's the recommended way to access the `EdenFsInstance`
    /// throughout your application.
    ///
    /// # Panics
    ///
    /// Panics if the global instance has not been initialized with [`init`](Self::init).
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// ```
    pub fn global() -> &'static EdenFsInstance {
        INSTANCE.get().expect("EdenFsInstance is not initialized")
    }

    /// Creates a new `EdenFsInstance` with the specified paths.
    ///
    /// This constructor creates a new instance without setting it as the global instance.
    /// If you want to initialize the global instance, use [`init`](Self::init) instead.
    ///
    /// This method is useful for testing or when you need to create multiple instances.
    ///
    /// # Parameters
    ///
    /// * `config_dir` - Path to the EdenFS configuration directory
    /// * `etc_eden_dir` - Path to the system-wide EdenFS configuration directory
    /// * `home_dir` - Optional path to the user's home directory
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// let instance = EdenFsInstance::new(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// ```
    pub fn new(
        config_dir: PathBuf,
        etc_eden_dir: PathBuf,
        home_dir: Option<PathBuf>,
    ) -> EdenFsInstance {
        Self {
            config_dir,
            etc_eden_dir,
            home_dir,
        }
    }

    /// Initializes the global `EdenFsInstance` with the specified paths.
    ///
    /// This method creates a new `EdenFsInstance` and sets it as the global instance
    /// that can be accessed with [`global`](Self::global).
    ///
    /// # Parameters
    ///
    /// * `config_dir` - Path to the EdenFS configuration directory
    /// * `etc_eden_dir` - Path to the system-wide EdenFS configuration directory
    /// * `home_dir` - Optional path to the user's home directory
    ///
    /// # Panics
    ///
    /// Panics if the global instance has already been initialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// // Initialize the global instance
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// ```
    pub fn init(config_dir: PathBuf, etc_eden_dir: PathBuf, home_dir: Option<PathBuf>) {
        event!(
            Level::TRACE,
            ?config_dir,
            ?etc_eden_dir,
            ?home_dir,
            "Creating EdenFsInstance"
        );
        INSTANCE
            .set(EdenFsInstance::new(config_dir, etc_eden_dir, home_dir))
            .expect("should be able to initialize EdenfsInstance")
    }

    /// Loads and returns the EdenFS configuration.
    ///
    /// This method loads the configuration from the system-wide and user-specific
    /// configuration files.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the `EdenFsConfig` if successful, or an error if
    /// the configuration could not be loaded.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// match instance.get_config() {
    ///     Ok(config) => {
    ///         println!("Successfully loaded EdenFS configuration");
    ///         // Use config...
    ///     }
    ///     Err(err) => {
    ///         eprintln!("Failed to load EdenFS configuration: {}", err);
    ///     }
    /// }
    /// ```
    pub fn get_config(&self) -> Result<EdenFsConfig> {
        edenfs_config::load_config(
            &self.etc_eden_dir,
            self.home_dir.as_ref().map(|x| x.as_ref()),
        )
    }

    /// Returns a reference to the user's home directory if available.
    ///
    /// # Returns
    ///
    /// Returns `Some(&PathBuf)` containing the path to the user's home directory if it was
    /// provided during initialization, or `None` if it wasn't provided.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// if let Some(home_dir) = instance.get_user_home_dir() {
    ///     println!("User home directory: {}", home_dir.display());
    /// }
    /// ```
    pub fn get_user_home_dir(&self) -> Option<&PathBuf> {
        self.home_dir.as_ref()
    }

    /// Creates and returns a new `EdenFsClient` for interacting with EdenFS.
    ///
    /// This method creates a new client that connects to the EdenFS daemon using the
    /// socket file path from this instance.
    ///
    /// # Returns
    ///
    /// Returns a new `EdenFsClient` instance.
    pub fn get_client(&self) -> EdenFsClient {
        EdenFsClient::new(expect_init(), self.socketfile(), None)
    }

    /// Creates and returns a new `StreamingEdenFsClient` for interacting with EdenFS.
    ///
    /// This method creates a new streaming client that connects to the EdenFS daemon using
    /// the socket file path from this instance. Streaming clients are useful for operations
    /// that return streams of data.
    ///
    /// # Returns
    ///
    /// Returns a new `StreamingEdenFsClient` instance.
    pub fn get_streaming_client(&self) -> StreamingEdenFsClient {
        StreamingEdenFsClient::new(expect_init(), self.socketfile(), None)
    }

    /// Returns the path to the EdenFS socket file.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS socket file.
    pub(crate) fn socketfile(&self) -> PathBuf {
        self.config_dir.join("socket")
    }

    /// Returns the path to the EdenFS socket file.
    ///
    /// If `check` is true, this method will verify that the socket file exists and
    /// return an error if it doesn't.
    ///
    /// # Parameters
    ///
    /// * `check` - Whether to check if the socket file exists
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the path to the socket file if successful, or an error
    /// if `check` is true and the socket file doesn't exist.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    ///
    /// // Get socket path without checking if it exists
    /// let socket_path = instance.get_socket_path(false).unwrap();
    /// println!("EdenFS socket path: {}", socket_path.display());
    ///
    /// // Get socket path and check if it exists
    /// match instance.get_socket_path(true) {
    ///     Ok(path) => println!("EdenFS socket exists at: {}", path.display()),
    ///     Err(err) => eprintln!("EdenFS socket issue: {}", err),
    /// }
    /// ```
    pub fn get_socket_path(&self, check: bool) -> Result<PathBuf, anyhow::Error> {
        let socketfile = self.socketfile();

        if check {
            if !std::fs::exists(&socketfile).with_context(|| {
                format!(
                    "Failed to check existence of socket file {}",
                    socketfile.display()
                )
            })? {
                return Err(anyhow!(
                    "EdenFS socket file {} doesn't exist on this machine",
                    socketfile.display()
                ));
            }
        }
        Ok(socketfile.to_owned())
    }

    /// Returns the path to the EdenFS PID file on Windows.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS PID file.
    #[cfg(windows)]
    fn pidfile(&self) -> PathBuf {
        self.config_dir.join("pid")
    }

    /// Returns the path to the EdenFS lock file on Unix systems.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS lock file.
    #[cfg(unix)]
    fn pidfile(&self) -> PathBuf {
        self.config_dir.join("lock")
    }

    /// Reads the process ID from the EdenFS lock file.
    ///
    /// This method reads the PID from the lock file and parses it as a `sysinfo::Pid`.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the PID if successful, or an error if the PID
    /// could not be read or parsed.
    ///
    /// # Errors
    ///
    /// This method can fail if:
    /// - The lock file does not exist
    /// - The lock file cannot be read
    /// - The lock file content is not valid UTF-8
    /// - The lock file content is not a valid PID
    fn pid(&self) -> Result<sysinfo::Pid, anyhow::Error> {
        let pidfile = self.pidfile();
        let pid_bytes = std::fs::read(&pidfile)
            .with_context(|| format!("Unable to read from pid file '{}'", pidfile.display()))?;
        let pid_str =
            std::str::from_utf8(&pid_bytes).context("Unable to parse pid file as UTF-8 string")?;

        pid_str
            .trim()
            .parse()
            .with_context(|| format!("Unable to parse pid file content: '{}'", pid_str))
    }

    /// Retrieves the running EdenFS process status based on the lock file.
    ///
    /// This method checks if the EdenFS process is running by reading the PID from the
    /// lock file and verifying that the process exists and is an EdenFS process.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the process status if successful, or an error if
    /// the process is not running or is not an EdenFS process.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// match instance.status_from_lock() {
    ///     Ok(_) => println!("EdenFS is running"),
    ///     Err(err) => println!("EdenFS status: {}", err),
    /// }
    /// ```
    pub fn status_from_lock(&self) -> Result<i32, anyhow::Error> {
        let pid = self.pid()?;

        let exe = match get_executable(pid) {
            Some(exe) => exe,
            None => {
                tracing::debug!("PID {} is not running", pid);
                return Err(anyhow!("EdenFS is not running"));
            }
        };
        let name = match exe.file_name() {
            Some(name) => name.to_string_lossy(),
            None => {
                tracing::debug!("Unable to retrieve information about PID {}", pid);
                return Err(anyhow!("EdenFS is not running"));
            }
        };

        tracing::trace!(?name, "executable name");

        if name == "edenfs"
            || name == "fake_edenfs"
            || (cfg!(windows) && name.ends_with("edenfs.exe"))
        {
            Err(anyhow!(
                "EdenFS's Thrift server does not appear to be running, \
                but the process is still alive (PID={})",
                pid
            ))
        } else {
            Err(anyhow!("EdenFS is not running"))
        }
    }

    /// Returns a map of mount paths to mount names as defined in EdenFS's config.json.
    ///
    /// This method reads the EdenFS configuration file and returns a map where the keys
    /// are the mount paths and the values are the mount names.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a `BTreeMap` mapping mount paths to mount names if
    /// successful, or an error if the configuration file could not be read or parsed.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// match instance.get_configured_mounts_map() {
    ///     Ok(mounts) => {
    ///         println!("Configured mounts:");
    ///         for (path, name) in mounts {
    ///             println!("  {} -> {}", path.display(), name);
    ///         }
    ///     }
    ///     Err(err) => {
    ///         eprintln!("Failed to get configured mounts: {}", err);
    ///     }
    /// }
    /// ```
    pub fn get_configured_mounts_map(&self) -> Result<BTreeMap<PathBuf, String>, anyhow::Error> {
        let directory_map = self.config_dir.join(CONFIG_JSON);
        match std::fs::read_to_string(&directory_map) {
            Ok(buff) => {
                let string_map = serde_json::from_str::<BTreeMap<String, String>>(&buff)
                    .with_context(|| format!("Failed to parse directory map: {:?}", &buff))?;
                Ok(string_map
                    .into_iter()
                    .map(|(key, val)| (key.into(), val))
                    .collect())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e)
                .with_context(|| format!("Failed to read directory map from {:?}", directory_map)),
        }
    }

    /// Returns the path to the EdenFS clients directory.
    ///
    /// This directory contains subdirectories for each client/mount managed by EdenFS.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS clients directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let clients_dir = instance.clients_dir();
    /// println!("Clients directory: {}", clients_dir.display());
    /// ```
    pub fn clients_dir(&self) -> PathBuf {
        self.config_dir.join(CLIENTS_DIR)
    }

    /// Returns the path to the EdenFS logs directory.
    ///
    /// This directory contains log files generated by EdenFS.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS logs directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let logs_dir = instance.logs_dir();
    /// println!("Logs directory: {}", logs_dir.display());
    /// ```
    pub fn logs_dir(&self) -> PathBuf {
        self.config_dir.join("logs")
    }

    /// Returns the path to the EdenFS storage directory.
    ///
    /// This directory contains storage-related files used by EdenFS.
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the EdenFS storage directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let storage_dir = instance.storage_dir();
    /// println!("Storage directory: {}", storage_dir.display());
    /// ```
    pub fn storage_dir(&self) -> PathBuf {
        self.config_dir.join("storage")
    }

    /// Returns the client name for a given path.
    ///
    /// This method resolves the path to an absolute path and finds the corresponding
    /// client name by checking if the path is a subpath of any configured mount point.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to get the client name for
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the client name if successful, or an error if
    /// the path is not handled by EdenFS.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let path = Path::new("/path/to/checkout");
    /// match instance.client_name(path) {
    ///     Ok(name) => println!("Client name for {}: {}", path.display(), name),
    ///     Err(err) => eprintln!("Failed to get client name: {}", err),
    /// }
    /// ```
    pub fn client_name(&self, path: &Path) -> Result<String> {
        // Resolve symlinks and get absolute path
        let path = path.canonicalize().from_err()?;
        #[cfg(windows)]
        let path = strip_unc_prefix(path);

        // Find `checkout_path` that `path` is a sub path of
        let all_checkouts = self.get_configured_mounts_map()?;
        if let Some(item) = all_checkouts
            .iter()
            .find(|&(checkout_path, _)| path.starts_with(checkout_path))
        {
            let (_, checkout_name) = item;
            Ok(checkout_name.clone())
        } else {
            Err(EdenFsError::Other(anyhow!(
                "Checkout path {} is not handled by EdenFS",
                path.display()
            )))
        }
    }

    /// Returns the configuration directory for a specific client.
    ///
    /// # Parameters
    ///
    /// * `client_name` - The name of the client
    ///
    /// # Returns
    ///
    /// Returns a `PathBuf` containing the path to the client's configuration directory.
    ///
    /// # Examples
    ///
    /// ```
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let client_name = "my_client";
    /// let config_dir = instance.config_directory(client_name);
    /// println!(
    ///     "Config directory for {}: {}",
    ///     client_name,
    ///     config_dir.display()
    /// );
    /// ```
    pub fn config_directory(&self, client_name: &str) -> PathBuf {
        self.clients_dir().join(client_name)
    }

    /// Returns the client directory for a given mount point.
    ///
    /// This method first determines the client name for the given path, then returns
    /// the path to the client's directory.
    ///
    /// # Parameters
    ///
    /// * `path` - The mount point path
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the path to the client's directory if successful,
    /// or an error if the path is not handled by EdenFS.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let mount_point = Path::new("/path/to/mount");
    /// match instance.client_dir_for_mount_point(mount_point) {
    ///     Ok(dir) => println!("Client directory: {}", dir.display()),
    ///     Err(err) => eprintln!("Failed to get client directory: {}", err),
    /// }
    /// ```
    pub fn client_dir_for_mount_point(&self, path: &Path) -> Result<PathBuf> {
        Ok(self.clients_dir().join(self.client_name(path)?))
    }

    /// Removes a path from the EdenFS directory map.
    ///
    /// This method acquires an exclusive lock on the configuration file, reads the
    /// current directory map, removes the specified path, and writes the updated
    /// map back to the file.
    ///
    /// # Parameters
    ///
    /// * `path` - The path to remove from the directory map
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating success or failure.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use edenfs_client::instance::EdenFsInstance;
    /// use edenfs_client::utils::get_config_dir;
    /// use edenfs_client::utils::get_etc_eden_dir;
    /// use edenfs_client::utils::get_home_dir;
    ///
    /// EdenFsInstance::init(
    ///     get_config_dir(&None, &None).unwrap(),
    ///     get_etc_eden_dir(&None),
    ///     get_home_dir(&None),
    /// );
    /// let instance = EdenFsInstance::global();
    /// let path = Path::new("/path/to/remove");
    /// match instance.remove_path_from_directory_map(path) {
    ///     Ok(_) => println!("Successfully removed path from directory map"),
    ///     Err(err) => eprintln!("Failed to remove path: {}", err),
    /// }
    /// ```
    pub fn remove_path_from_directory_map(&self, path: &Path) -> Result<()> {
        let lock_file_path = self.config_dir.join(CONFIG_JSON_LOCK);
        let config_file_path = self.config_dir.join(CONFIG_JSON);

        // For Linux and MacOS we have a lock file "config.json.lock" under the config directory
        // which works as a file lock to prevent the file "config.json" being accessed by
        // multiple processes at the same time.
        //
        // In Python CLI code, FileLock lib is used to create config.json.lock.
        // In Rust, we use PathLock from "scm/lib/util"
        let _lock = PathLock::exclusive(&lock_file_path).with_context(|| {
            format!("Failed to open the lock file {}", lock_file_path.display())
        })?;

        // Lock acquired, now we can read and write to the "config.json" file

        // On Windows the "Path" crate will append the prefix "\\?\" to the original path when
        // "canonicalize()" is called to indicate the path is in unicode.
        // We need to strip the prefix before checking the key in "config.json" file
        // For non-windows platforms, this is no-op.
        let entry_key = dunce::simplified(path);
        let mut all_checkout_map = self.get_configured_mounts_map()?;
        let original_num_of_entries = all_checkout_map.len();

        all_checkout_map.retain(|path, _| dunce::simplified(path) != entry_key);

        if all_checkout_map.len() < original_num_of_entries {
            atomic_write(&config_file_path, CONFIG_JSON_MODE, true, |f| {
                serde_json::to_writer_pretty(f, &all_checkout_map)?;
                Ok(())
            })
            .with_context(|| {
                format!(
                    "Failed to write updated config JSON back to {}",
                    config_file_path.display()
                )
            })?;
        } else {
            event!(
                Level::WARN,
                "There is not entry for {} in config.json",
                path.display()
            );
        }

        // Lock will be released when _lock is dropped
        Ok(())
    }
}
