use {
    crate::{
        layer::Layer,
        r#type::{
            LayerId, LayerName, ObjectName, RawTime, SessionId, SessionInfo, SessionName,
            TimeFormat, Color,
        },
        storage::{self, StorageManager, StorageTransaction},
        transaction, Result,
    },
    kiss3d::{scene::SceneNode, window::Window, camera::Camera},
    lazy_static::lazy_static,
    log::{error, trace, warn},
    ptree::{item::StringItem, TreeBuilder},
    std::sync::{mpsc, Arc},
    nalgebra::Point2,
};

pub mod actor;
pub mod context;
pub mod phys;
pub mod math;
pub mod scene;

use context::{Context, TimeRange, ContextChangeParams};
use scene::Scene;

const CONNECTION_STRING: &'static str = "host=localhost user=postgres";
const LOG_TARGET: &'static str = "engine";

const CONTEXT_CHANGE_RATIO: f32 = 0.6;

lazy_static! {
    static ref ACCESS_UPDATE_TIME: chrono::Duration = chrono::Duration::seconds(30);
    pub static ref SESSION_MAX_HANG_TIME: chrono::Duration =
        chrono::Duration::seconds(ACCESS_UPDATE_TIME.num_seconds() + 10);
}

pub struct DebugInfoSettings {
    pub tracks: Option<chrono::Duration>,
    pub names: bool,
    pub show_rtree: bool,
    pub sim_stats: bool,
}

pub struct Engine {
    storage_mgr: StorageManager,
    context: Arc<Context>,
    context_recv: mpsc::Receiver<Context>,
    context_upd_intrp: mpsc::Sender<()>,
    scene: Scene,
    real_time: chrono::Duration,
    last_session_update_time: chrono::Duration,
    virtual_time: chrono::Duration,
    virtual_step: chrono::Duration,
    last_frame_delta: chrono::Duration,
    frames_sum_time_ms: usize,
    frame_count: usize,
    context_change_params: Option<ContextChangeParams>,
    is_context_change_spawned: bool,
    debug_info_settings: DebugInfoSettings,
}

impl Engine {
    pub fn init(root_scene_node: SceneNode) -> Result<Self> {
        let storage_mgr = StorageManager::setup(CONNECTION_STRING, *SESSION_MAX_HANG_TIME)?;
        let (_, context_recv) = mpsc::channel();
        let (context_upd_intrp, _) = mpsc::channel();

        let mut engine = Self {
            storage_mgr,
            context: Arc::new(Context::new(SessionId::default(), LayerId::default())),
            context_recv,
            context_upd_intrp,
            scene: Scene::new(root_scene_node),
            real_time: chrono::Duration::zero(),
            last_session_update_time: chrono::Duration::zero(),
            virtual_time: chrono::Duration::zero(),
            virtual_step: chrono::Duration::seconds(1),
            last_frame_delta: chrono::Duration::zero(),
            frames_sum_time_ms: 0,
            frame_count: 0,
            context_change_params: None,
            is_context_change_spawned: false,
            debug_info_settings: DebugInfoSettings {
                tracks: None,
                names: false,
                show_rtree: false,
                sim_stats: true,
            },
        };

        let session_name = None;
        let old_session_id = None;

        engine.new_session_helper(session_name, old_session_id)?;
        engine.start_context_change()?;
        let context = engine.context_recv.recv().expect("unable to load initial context");
        engine.set_new_context(context)?;

        Ok(engine)
    }

    pub fn advance_time(&mut self, frame_delta_ns: i128, advance_virtual_time: bool) -> Result<()> {
        match self.context_recv.try_recv() {
            Ok(new_context) => self.set_new_context(new_context)?,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.start_context_change()?;
            }
            _ => {}
        }

        let one_second_ns = chrono::Duration::seconds(1).num_nanoseconds().unwrap() as i128;
        let ns_per_ms = 1_000_000;

        let vt_step_ns = self.virtual_step.num_milliseconds() as i128 * ns_per_ms;
        let real_step = (frame_delta_ns as i128 * vt_step_ns / one_second_ns) as RawTime;

        if advance_virtual_time {
            self.virtual_time = self.virtual_time + chrono::Duration::nanoseconds(real_step);
            self.scene.set_time(&self.context, self.virtual_time);

            if !self.is_context_change_spawned
            && self.context().time_range().ratio(self.virtual_time) >= CONTEXT_CHANGE_RATIO {
                self.schedule_context_change(
                    self.context().session_id(),
                    self.context().layer_id(),
                    TimeRange::with_default_len(self.virtual_time),
                );
            }
        }

        self.last_frame_delta =
            chrono::Duration::milliseconds((frame_delta_ns / ns_per_ms) as RawTime);
        self.real_time = self.real_time + self.last_frame_delta;

        self.frames_sum_time_ms += (frame_delta_ns / ns_per_ms) as usize;
        self.frame_count += 1;

        self.update_session_access_time().unwrap_or_else(|err| {
            error! {
                target: LOG_TARGET,
                "unable to update the session access time: {}",
                err
            }
        });

        Ok(())
    }

    pub fn draw_debug_info<C: Camera>(&mut self, window: &mut Window, camera: &mut C) {
        let context = self.context.as_ref();
        let settings = &self.debug_info_settings;

        self.scene.draw_debug_info(window, camera, context, settings, self.virtual_time);

        if settings.sim_stats {
            self.draw_simulation_stats(window);
        }
    }

    pub fn compute_locations(&mut self) {}

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub fn virtual_time(&self) -> chrono::Duration {
        self.virtual_time
    }

    pub fn set_virtual_time(
        &mut self,
        mut vtime: chrono::Duration,
        try_current_context: bool,
    ) {
        if vtime < chrono::Duration::zero() {
            vtime = chrono::Duration::zero();
        }

        self.virtual_time = vtime;

        if try_current_context && self.context().time_range().contains(vtime) {
            self.scene.set_time(self.context.as_ref(), vtime);

            return;
        }

        self.schedule_context_change(
            self.context().session_id(),
            self.context().layer_id(),
            TimeRange::with_default_len(vtime),
        );
    }

    pub fn virtual_step(&self) -> chrono::Duration {
        self.virtual_step
    }

    pub fn set_virtual_step(&mut self, vstep: chrono::Duration) {
        self.virtual_step = vstep;
    }

    pub fn frame_avg_time_ms(&self) -> f32 {
        self.frames_sum_time_ms as f32 / self.frame_count as f32
    }

    pub fn toggle_rtree(&mut self) {
        self.debug_info_settings.show_rtree = !self.debug_info_settings.show_rtree;
    }

    pub fn show_tracks(&mut self, track_step: chrono::Duration) {
        self.debug_info_settings.tracks = Some(track_step);
    }

    pub fn hide_tracks(&mut self) {
        self.debug_info_settings.tracks = None;
    }

    pub fn toggle_names(&mut self) {
        self.debug_info_settings.names = !self.debug_info_settings.names;
    }

    pub fn toggle_stats(&mut self) {
        self.debug_info_settings.sim_stats = !self.debug_info_settings.sim_stats;
    }

    fn draw_simulation_stats(&mut self, window: &mut Window) {
        use std::fmt::Write;

        let pos = Point2::new(0.0, 150.0);

        let mut stats_text = String::new();

        writeln!(&mut stats_text, "frame #{}", self.frame_count).unwrap();

        writeln!(
            &mut stats_text,
            "virtual time: {}",
            TimeFormat::VirtualTimeLong(self.virtual_time)
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "virtual time step: {}",
            TimeFormat::VirtualTimeShort(self.virtual_step)
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "frame delta time: {}",
            TimeFormat::FrameDelta(self.last_frame_delta)
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "frame avg ms: {}",
            self.frame_avg_time_ms()
        )
        .unwrap();

        self.scene.draw_text(window, &stats_text, pos, Color::new(1.0, 0.0, 1.0));

        if self.is_context_change_spawned {
            let upd_text = format!("*UPDATING CONTEXT*");

            let pos = Point2::new(pos[0], pos[1] + 450.0);

            self.scene.draw_text(window, &upd_text, pos, Color::new(1.0, 1.0, 0.0));
        }
    }

    pub fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }

    pub fn add_layer(&mut self, layer: Layer) -> Result<()> {
        let session_id = self.context.session_id();
        let active_layer_id = self.context.layer_id();
        let new_layer_start_time = self.virtual_time;

        let new_layer_id;

        transaction! {
            self.storage_mgr => t(RepeatableRead) {
                new_layer_id = t.layer().add_layer(
                    session_id,
                    active_layer_id,
                    layer.name(),
                    new_layer_start_time
                )?;

                for (object, coord) in layer.take_objects() {
                    let object_id = t.object().add(session_id, new_layer_id, object)?;
                    t.location().add(object_id, new_layer_id, coord)?;
                }
            }
        }

        self.select_layer_helper(new_layer_id);

        Ok(())
    }

    pub fn is_object_exists(&mut self, object_name: &ObjectName) -> Result<bool> {
        let result;
        transaction! {
            self.storage_mgr => t {
                result = t.object().is_object_exists(self.context.session_id(), object_name);
            }
        }

        result
    }

    pub fn get_layer_id(&mut self, layer_name: &LayerName) -> Result<LayerId> {
        let result;

        transaction! {
            self.storage_mgr => t {
                result = t.layer().get_layer_id(self.context.session_id(), layer_name);
            }
        }

        result
    }

    pub fn remove_layer(&mut self, layer_name: &LayerName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                match self.get_layer_id(layer_name) {
                    Ok(layer_id) => {
                        let mut layer = t.layer();
                        let active_ancestors = layer.layer_ancestors(self.context.layer_id())?;

                        if active_ancestors.contains(&layer_id) {
                            error! {
                                target: LOG_TARGET,
                                "unable to remove active layer or it's ancestors"
                            }
                        } else {
                            layer.remove_layer(layer_id)?;
                        }
                    },
                    Err(err) => warn!("unable to remove a layer: {}", err)
                }
            }
        }

        Ok(())
    }

    pub fn rename_layer(
        &mut self,
        old_layer_name: &LayerName,
        new_layer_name: &LayerName,
    ) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                let mut layer = t.layer();

                let id = layer.get_layer_id(self.context.session_id(), old_layer_name)?;

                layer.rename_layer(id, new_layer_name)?;
            }
        }

        Ok(())
    }

    pub fn active_layer_name(&mut self) -> Result<LayerName> {
        let id = self.context.layer_id();

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
        layer_api.get_current_layer_id(self.context.layer_id(), self.virtual_time)
    }

    pub fn get_session_layers(&mut self) -> Result<StringItem> {
        let result;
        transaction! {
            self.storage_mgr => t {
                let session_id = self.context.session_id();
                let session_name = t.session().get_name(session_id)?;

                let tree_title = format!("layers of the session \"{}\"", session_name);
                let mut builder = TreeBuilder::new(tree_title);

                let mut layer = t.layer();
                let current_layer_id = self.current_layer_id(&mut layer)?;
                let parent_layer_id = layer.get_main_layer(session_id)?;

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
        parent_layer_id: LayerId,
    ) -> Result<()> {
        let start_time = layer_api.get_start_time(parent_layer_id)?;

        let active_layer_id = self.context().layer_id();
        let layer_name = layer_api.get_name(parent_layer_id)?;
        let layer_status =
            if parent_layer_id == active_layer_id && parent_layer_id == current_layer_id {
                "[active/current] "
            } else if parent_layer_id == active_layer_id {
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

        let children = layer_api.get_layer_children(self.context.session_id(), parent_layer_id)?;

        // if children.is_empty() {
        //     builder.add_empty_child(layer_info);
        // } else {
        builder.begin_child(layer_info);

        for &child_id in children.iter() {
            self.get_session_layers_helper(layer_api, builder, current_layer_id, child_id)?;
        }

        builder.end_child();
        // }

        Ok(())
    }

    pub fn select_layer(&mut self, layer_name: &LayerName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                let layer_id = t.layer().get_layer_id(self.context.session_id(), layer_name)?;

                self.select_layer_helper(layer_id);
            }
        }

        Ok(())
    }

    fn select_layer_helper(&mut self, layer_id: LayerId) {
        // self.spawn_context_change(
        //     self.context.session_id(),
        //     layer_id,
        //     self.context().time_range().clone(),
        // )
        self.schedule_context_change(
            self.context.session_id(),
            layer_id,
            self.context().time_range().clone(),
        );
    }

    pub fn get_session_name(&mut self) -> Result<SessionName> {
        let result;
        transaction! {
            self.storage_mgr => t {
                result = t.session().get_name(self.context.session_id());
            }
        }

        result
    }

    pub fn get_sessions_info(&mut self) -> Result<Vec<SessionInfo>> {
        let result;
        transaction! {
            self.storage_mgr => t {
                result = t.session().get_list();
            }
        }

        result
    }

    pub fn new_session(&mut self, session_name: Option<SessionName>) -> Result<()> {
        self.new_session_helper(session_name, Some(self.context.session_id()))
    }

    pub fn save_session(&mut self, session_name: SessionName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                t.session().save(self.context.session_id(), &session_name)?;
            }
        }

        Ok(())
    }

    /// Returns max object's default name number
    pub fn load_session(&mut self, session_name: SessionName) -> Result<i64> {
        let num;

        transaction! {
            self.storage_mgr => t {
                let mut session = t.session();
                let (new_session_id, new_layer_id) = session.load(&session_name)?;

                self.set_new_session(
                    &mut session,
                    new_session_id,
                    new_layer_id,
                    Some(self.context.session_id())
                )?;

                num = t.object().get_max_object_default_name_num(new_session_id)?;
            }
        }

        Ok(num)
    }

    pub fn rename_session(
        &mut self,
        old_session_name: SessionName,
        new_session_name: SessionName,
    ) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                t.session().rename(&old_session_name, &new_session_name)?;
            }
        }

        Ok(())
    }

    pub fn delete_session(&mut self, session_name: SessionName) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                t.session().delete(&session_name)?;
            }
        }

        Ok(())
    }

    fn new_session_helper(
        &mut self,
        session_name: Option<SessionName>,
        old_session_id: Option<SessionId>,
    ) -> Result<()> {
        transaction! {
            self.storage_mgr => t {
                let mut session = t.session();

                let (new_session_id, new_layer_id) = session.new(session_name)?;

                self.set_new_session(&mut session, new_session_id, new_layer_id, old_session_id)?;
            }
        }

        Ok(())
    }

    fn set_new_session(
        &mut self,
        session: &mut storage::Session,
        new_session_id: SessionId,
        new_layer_id: LayerId,
        old_session_id: Option<SessionId>,
    ) -> Result<()> {
        if let Some(old_session_id) = old_session_id {
            session.unlock(old_session_id)?;
        }

        // self.spawn_context_change(new_session_id, new_layer_id, TimeRange::default())
        self.schedule_context_change(new_session_id, new_layer_id, TimeRange::default());

        Ok(())
    }

    fn schedule_context_change(
        &mut self,
        session_id: SessionId,
        layer_id: LayerId,
        time_range: TimeRange
    ) {
        if let Ok(_) = self.context_upd_intrp.send(()) {
            trace! {
                target: LOG_TARGET,
                "interrupt context update"
            }
        }

        let change_params = ContextChangeParams {
            session_id,
            layer_id,
            time_range,
        };

        self.context_change_params = Some(change_params);
    }

    fn start_context_change(&mut self) -> Result<()> {
        if self.context_change_params.is_none() {
            return Ok(());
        }

        let mut change_params = self.context_change_params.clone().unwrap();

        if change_params.session_id == self.context.session_id()
        && change_params.layer_id == self.context.layer_id()
        && change_params.time_range == self.context.time_range().clone() {
            self.context_change_params = None;

            return Ok(());
        }

        trace! {
            target: LOG_TARGET,
            "start context change"
        }

        self.is_context_change_spawned = true;

        let min_valid_start_time;
        transaction! {
            self.storage_mgr => t {
                min_valid_start_time = t.location()
                    .get_min_valid_start_time(change_params.layer_id, change_params.time_range.start())?;
            }
        }

        if min_valid_start_time < change_params.time_range.start() {
            change_params.time_range = TimeRange::with_default_len(min_valid_start_time);
        }

        let (ctx_sender, ctx_recv) = mpsc::channel();
        let (ctx_upd_intrp_sender, ctx_upd_intrp_recv) = mpsc::channel();

        self.context_recv = ctx_recv;
        self.context_upd_intrp = ctx_upd_intrp_sender;

        let storage_mgr = self.storage_mgr.clone();
        let context = Arc::clone(&self.context);

        rayon::spawn(move || {
            let (new_context, update_kind) = context.replicate(
                change_params.session_id,
                change_params.layer_id,
                change_params.time_range
            );

            if let Ok(new_context) = new_context.update_content(
                storage_mgr,
                update_kind,
                ctx_upd_intrp_recv
            ) {
                match ctx_sender.send(new_context) {
                    Ok(_) => trace! {
                        target: LOG_TARGET,
                        "new context is sent"
                    },
                    Err(err) => error! {
                        target: LOG_TARGET,
                        "[context] {}", err
                    },
                }
            }
        });

        Ok(())
    }

    fn set_new_context(&mut self, mut context: Context) -> Result<()> {
        if self.context.session_id() != context.session_id()
            || self.context().layer_id() != context.layer_id()
        {
            self.scene.clear();
        }

        self.scene.update(&mut context);
        self.scene.set_time(&context, self.virtual_time);

        self.context = Arc::new(context);

        trace! {
            target: LOG_TARGET,
            "context is changed"
        }

        // if let Some(target_time_range) = self.target_time_range.take() {
        //     return self.spawn_context_change(
        //         self.context.session_id(),
        //         self.context.layer_id(),
        //         target_time_range,
        //     );
        // }

        self.is_context_change_spawned = false;

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
                    t.session().update_access_time(self.context.session_id())?;
                }
            }

            self.last_session_update_time = self.real_time;
        }

        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let mut pooled_connection = self
            .storage_mgr
            .pool
            .get()
            .expect("the pooled connection is expected to be established");
        let mut transaction = pooled_connection
            .transaction()
            .expect("the transaction is expected to be started");

        let mut session = storage::Session::new_api(&mut transaction);

        session
            .unlock(self.context.session_id())
            .expect("the session is expected to be unlocked");

        transaction.commit().unwrap();
    }
}
