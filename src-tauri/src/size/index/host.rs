use std::sync::{Arc, Mutex, MutexGuard};

use super::service::{FolderSizeIndex, IndexOptions, StatusEmitter};
use crate::size::{FolderSizeIndexStatus, IndexBackend, IndexScope, IndexState};

pub type IndexOptionsFactory = Box<dyn Fn() -> IndexOptions + Send + Sync>;

pub struct IndexHostSetup {
    pub options_for_start: IndexOptionsFactory,
    pub emit_status: StatusEmitter,
    pub scope: IndexScope,
}

struct Running {
    scope: IndexScope,
    index: Option<Arc<FolderSizeIndex>>,
}

impl Running {
    fn stopped() -> Self {
        Self {
            scope: IndexScope::Off,
            index: None,
        }
    }
}

pub struct IndexHost {
    options_for_start: IndexOptionsFactory,
    emit_status: StatusEmitter,
    running: Mutex<Running>,
    switching: Mutex<()>,
}

impl IndexHost {
    pub fn start(setup: IndexHostSetup) -> Self {
        let host = Self {
            options_for_start: setup.options_for_start,
            emit_status: setup.emit_status,
            running: Mutex::new(Running::stopped()),
            switching: Mutex::new(()),
        };
        host.set_scope(setup.scope);
        host
    }

    pub fn index(&self) -> Option<Arc<FolderSizeIndex>> {
        self.running().index.clone()
    }

    pub fn scope(&self) -> IndexScope {
        self.running().scope
    }

    pub fn status(&self) -> FolderSizeIndexStatus {
        self.index().map_or_else(off_status, |index| index.status())
    }

    pub fn set_scope(&self, scope: IndexScope) {
        let _switching = self.switching.lock().expect("index switch lock");
        if self.scope() == scope {
            return;
        }
        self.stop_running();
        if scope == IndexScope::Off {
            log::info!("size index: turned off");
            (self.emit_status)(off_status());
            return;
        }
        let index = self.started(scope);
        *self.running() = Running {
            scope,
            index: Some(index),
        };
    }

    fn stop_running(&self) {
        let stopped = std::mem::replace(&mut *self.running(), Running::stopped());
        if let Some(index) = stopped.index {
            index.stop();
        }
    }

    fn started(&self, scope: IndexScope) -> Arc<FolderSizeIndex> {
        let options = IndexOptions {
            collect_file_names: scope.collects_file_names(),
            ..(self.options_for_start)()
        };
        Arc::new(FolderSizeIndex::start(options))
    }

    fn running(&self) -> MutexGuard<'_, Running> {
        self.running.lock().expect("index host lock")
    }
}

fn off_status() -> FolderSizeIndexStatus {
    FolderSizeIndexStatus::new(IndexBackend::MacIndex, false, IndexState::Off)
}
