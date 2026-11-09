use std::sync::Arc;

use tauri::ipc::{CommandArg, CommandItem, InvokeError};
use tauri::{Runtime, State};

use crate::directory_session::DirectorySessionService;
use crate::volumes::registry::VolumeRegistry;
#[cfg(not(feature = "test-utils"))]
use crate::{ipc::executor::IpcExecutor, item_counts::ItemCountService, watch::WatchService};

fn managed_state<'de, T, R>(command: &CommandItem<'de, R>) -> Result<State<'de, T>, InvokeError>
where
    T: Send + Sync + 'static,
    R: Runtime,
{
    State::from_command(CommandItem {
        plugin: command.plugin,
        name: command.name,
        key: command.key,
        message: command.message,
        acl: command.acl,
    })
}

pub struct NavigationServices<'r> {
    pub sessions: State<'r, Arc<DirectorySessionService>>,
    pub registry: State<'r, Arc<VolumeRegistry>>,
}

impl<'r, 'de: 'r, R: Runtime> CommandArg<'de, R> for NavigationServices<'r> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        Ok(Self {
            sessions: managed_state(&command)?,
            registry: managed_state(&command)?,
        })
    }
}

#[cfg(not(feature = "test-utils"))]
pub struct ItemCountServices<'r> {
    pub item_counts: State<'r, ItemCountService>,
    pub sessions: State<'r, Arc<DirectorySessionService>>,
    pub executor: State<'r, Arc<IpcExecutor>>,
}

#[cfg(not(feature = "test-utils"))]
impl<'r, 'de: 'r, R: Runtime> CommandArg<'de, R> for ItemCountServices<'r> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        Ok(Self {
            item_counts: managed_state(&command)?,
            sessions: managed_state(&command)?,
            executor: managed_state(&command)?,
        })
    }
}

#[cfg(not(feature = "test-utils"))]
pub struct WatchServices<'r> {
    pub watches: State<'r, Arc<WatchService>>,
    pub sessions: State<'r, Arc<DirectorySessionService>>,
}

#[cfg(not(feature = "test-utils"))]
impl<'r, 'de: 'r, R: Runtime> CommandArg<'de, R> for WatchServices<'r> {
    fn from_command(command: CommandItem<'de, R>) -> Result<Self, InvokeError> {
        Ok(Self {
            watches: managed_state(&command)?,
            sessions: managed_state(&command)?,
        })
    }
}
