use {
    crate::{
        Result,
        transaction,
        storage::{self, StorageManager, StorageTransaction},
        layer::Layer,
        r#type::{
            RawTime,
            SessionId,
            SessionName,
            LayerId,
            LayerName,
            TimeFormat,
        }
    },
    lazy_static::lazy_static,
    log::{trace, error},
    ptree::{item::StringItem, TreeBuilder},
};

const CONNECTION_STRING: &'static str = "host=localhost user=postgres";
const LOG_TARGET: &'static str = "engine";

lazy_static! {
    static ref ACCESS_UPDATE_TIME: chrono::Duration = chrono::Duration::seconds(30);
    pub static ref SESSION_MAX_HANG_TIME: chrono::Duration =
        chrono::Duration::seconds(ACCESS_UPDATE_TIME.num_seconds() + 10);
}

pub struct Engine {
    storage_mgr: StorageManager,
    session_id: SessionId,
    active_layer_id: LayerId,
    real_time: chrono::Duration,
    last_session_update_time: chrono::Duration,
    virtual_time: chrono::Duration,
    virtual_step: chrono::Duration,
    last_frame_delta: chrono::Duration,
    frames_sum_time_ms: usize,
    frame_count: usize,
}

impl Engine {
    pub fn init() -> Result<Self> {
        let storage_mgr = StorageManager::setup(
            CONNECTION_STRING,
            *SESSION_MAX_HANG_TIME
        )?;

        let session_id = 0;
        let active_layer_id = 0;

        let mut engine = Self {
            storage_mgr,
            session_id,
            active_layer_id,
            real_time: chrono::Duration::zero(),
            last_session_update_time: chrono::Duration::zero(),
            virtual_time: chrono::Duration::zero(),
            virtual_step: chrono::Duration::seconds(1),
            last_frame_delta: chrono::Duration::zero(),
            frames_sum_time_ms: 0,
            frame_count: 0,
        };

        let session_name = None;
        let old_session_id = None;
        engine.new_session_helper(session_name, old_session_id)?;

        Ok(engine)
    }

    pub fn advance_time(&mut self, frame_delta_ns: RawTime, advance_virtual_time: bool) {
        let one_second_ns = chrono::Duration::seconds(1).num_nanoseconds().unwrap();
        let ns_per_ms = 1_000_000;

        let vt_step_ns = self.virtual_step.num_nanoseconds().unwrap();
        let real_step = frame_delta_ns * vt_step_ns / one_second_ns;

        if advance_virtual_time {
            self.virtual_time = self.virtual_time + chrono::Duration::nanoseconds(real_step);
        }

        self.last_frame_delta = chrono::Duration::nanoseconds(frame_delta_ns);
        self.real_time = self.real_time + self.last_frame_delta;

        self.frames_sum_time_ms += (frame_delta_ns / ns_per_ms) as usize;
        self.frame_count += 1;

        self.update_session_access_time().unwrap_or_else(|err| error! {
            target: LOG_TARGET,
            "unable to update the session access time: {}",
            err
        });
    }

    pub fn compute_locations(&mut self) {

    }

    pub fn virtual_time(&self) -> chrono::Duration {
        self.virtual_time
    }

    pub fn set_virtual_time(&mut self, vtime: chrono::Duration) {
        // TODO load from DB if needed

        self.virtual_time = vtime;
    }

    pub fn virtual_step(&self) -> chrono::Duration {
        self.virtual_step
    }

    pub fn set_virtual_step(&mut self, vstep: chrono::Duration) {
        self.virtual_step = vstep;
    }

    pub fn last_frame_delta(&self) -> chrono::Duration {
        self.last_frame_delta
    }

    pub fn frame(&self) -> usize {
        self.frame_count
    }

    pub fn frame_avg_time_ms(&self) -> f32 {
        self.frames_sum_time_ms as f32 / self.frame_count as f32
    }

    pub fn add_layer(&mut self, layer: Layer) -> Result<()> {
        let session_id = self.session_id;
        let active_layer_id = self.active_layer_id;
        let new_layer_start_time = self.virtual_time;

        let result;

        transaction! {
            self.storage_mgr => t {
                result = t.layer().add_layer(
                    session_id,
                    active_layer_id,
                    layer.name(),
                    new_layer_start_time
                );
            }
        }

        self.active_layer_id = result?;

        // todo!(
        //     "object addition"
        // );

        Ok(())
    }

    pub fn remove_layer(&mut self, layer_name: LayerName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                t.layer().remove_layer(self.session_id, layer_name)?;
            }
        }

        Ok(())
    }

    pub fn is_layer_exists(&mut self, name: &LayerName) -> Result<bool> {
        let result;

        transaction! {
            self.storage_mgr => t {
                result = t.layer().is_layer_exists(self.session_id, name);
            }
        }

        result
    }

    pub fn active_layer_name(&mut self) -> Result<LayerName> {
        let id = self.active_layer_id;

        let result;
        transaction! {
            self.storage_mgr => t {
                result = t.layer().get_name(id);
            }
        }

        result
    }

    pub fn current_layer_name(&mut self) -> Result<LayerName> {
        let result;
        transaction! {
            self.storage_mgr => t {
                let mut layer = t.layer();

                let id = self.current_layer_id(&mut layer)?;

                result = layer.get_name(id);
            }
        }

        result
    }

    fn current_layer_id(&mut self, layer_api: &mut storage::Layer) -> Result<LayerId> {
        layer_api.get_current_layer_id(self.active_layer_id, self.virtual_time)
    }

    pub fn get_session_layers(&mut self) -> Result<StringItem> {
        let result;
        transaction! {
            self.storage_mgr => t {
                let session_name = t.session().get_name(self.session_id)?;

                let tree_title = format!("layers of the session \"{}\"", session_name);
                let mut builder = TreeBuilder::new(tree_title);

                let mut layer = t.layer();
                let current_layer_id = self.current_layer_id(&mut layer)?;
                let parent_layer_id = layer.get_main_layer(self.session_id)?;

                self.get_session_layers_helper(
                    &mut layer,
                    &mut builder,
                    current_layer_id,
                    parent_layer_id
                )?;

                result = builder.build();
            }
        }

        Ok(result)
    }

    fn get_session_layers_helper(
        &mut self,
        layer_api: &mut storage::Layer,
        builder: &mut TreeBuilder,
        current_layer_id: LayerId,
        parent_layer_id: LayerId
    ) -> Result<()> {
        let start_time = layer_api.get_start_time(parent_layer_id)?;

        let layer_name = layer_api.get_name(parent_layer_id)?;
        let layer_status = if parent_layer_id == self.active_layer_id && parent_layer_id == current_layer_id {
            "[active/current] "
        } else if parent_layer_id == self.active_layer_id {
            "[active] "
        } else if parent_layer_id == current_layer_id {
            "[current] "
        } else {
            ""
        };

        let layer_info = format!(
            "{}{}: {}",
            layer_status,
            layer_name,
            TimeFormat::VirtualTimeShort(start_time)
        );

        let children = layer_api.get_layer_children(self.session_id, parent_layer_id)?;

        if children.is_empty() {
            builder.add_empty_child(layer_info);
        } else {
            builder.begin_child(layer_info);

            for &child_id in children.iter() {
                self.get_session_layers_helper(
                    layer_api,
                    builder,
                    current_layer_id,
                    child_id
                )?;
            }

            builder.end_child();
        }

        Ok(())
    }

    pub fn select_layer(&mut self, layer_name: LayerName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                let layer_id = t.layer().get_layer_id(self.session_id, layer_name)?;

                if self.active_layer_id != layer_id {
                    self.active_layer_id = layer_id;

                    self.request_simulation_info()?;
                }
            }
        }

        Ok(())
    }

    // pub fn new_se

    fn new_session_helper(
        &mut self,
        session_name: Option<SessionName>,
        old_session_id: Option<SessionId>
    ) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                let mut session = t.session();

                let (new_session_id, new_layer_id) = session.new(session_name)?;

                if let Some(old_session_id) = old_session_id {
                    session.unlock(old_session_id)?;
                }

                self.session_id = new_session_id;
                self.active_layer_id = new_layer_id;
            }
        }

        Ok(())
    }

    fn update_session_access_time(&mut self) -> Result<()> {
        if self.real_time.num_milliseconds()
            >= (self.last_session_update_time.num_milliseconds()
                + ACCESS_UPDATE_TIME.num_milliseconds())
        {
            trace! {
                target: LOG_TARGET,
                "update session access time"
            };

            transaction! {
                self.storage_mgr => t {
                    t.session().update_access_time(self.session_id)?;
                }
            }

            self.last_session_update_time = self.real_time;
        }

        Ok(())
    }

    fn request_simulation_info(&mut self) -> Result<()> {
        // TODO load from DB
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let mut pooled_connection = self.storage_mgr.pool.get()
            .expect("the pooled connection is expected to be established");
        let mut transaction = pooled_connection.transaction()
            .expect("the transaction is expected to be started");

        let mut session = storage::Session::new_api(&mut transaction);

        session.unlock(self.session_id)
            .expect("the session is expected to be unlocked");

        transaction.commit().unwrap();
    }
}