use std::{borrow::Borrow, cmp::Ordering, collections::BTreeSet};

use egui::{scroll_area::ScrollAreaOutput, Id, Pos2};
use hashbrown::HashMap;

#[derive(Default)]
pub struct Drag {
    state: Option<DragState>,
    all: HashMap<egui::Id, bool>, // pulse. whether the registered id is being used this frame
    horizontal: BTreeSet<DragId>,
    vertical: BTreeSet<DragId>,
    unique_id: u64,
}

impl Drag {
    /// Register the id to be recognized by the drag system.
    /// After initially registered, it will emit a pulse every frame.
    /// When the pulse stops (ie, this method is no longer being called for the id), it will be removed from the `Drag` system.
    /// This effectively means we will stop caring about it once the drag widget isn't being rendered anymore.
    pub fn register_id<R>(
        &mut self,
        id: IdType<'_, R>,
        direction: DragDirection,
        priority: DragPriority,
    ) {
        let drag_id = match id {
            IdType::ScrollArea(scroll_area_output) => scroll_area_output.id.with("area"),
            IdType::CustomDragId(id) => id,
        };

        if let Some(pulse) = self.all.get_mut(&drag_id) {
            *pulse = true;
            return;
        }

        self.all.insert(drag_id, true);

        let cache = match direction {
            DragDirection::Horizontal => &mut self.horizontal,
            DragDirection::Vertical => &mut self.vertical,
        };

        cache.insert(DragId {
            id: drag_id,
            priority,
            order_id: {
                self.unique_id += 1;
                self.unique_id
            },
            id_raw: drag_id.value(),
        });
    }

    pub fn register_highest_vertical_scroll<R>(&mut self, output: &ScrollAreaOutput<R>) {
        self.register_id(
            IdType::ScrollArea(output),
            DragDirection::Vertical,
            DragPriority::Highest,
        );
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        self.remove_all_no_pulse();
        self.update_internal(ctx);
        self.set_all_no_pulse();
    }

    fn remove_all_no_pulse(&mut self) {
        let all = &self.all;
        self.horizontal.retain(|d| *all.get(&d.id).unwrap_or(&true));
        self.vertical.retain(|d| *all.get(&d.id).unwrap_or(&true));
        self.all.retain(|_, v| *v);
    }

    fn set_all_no_pulse(&mut self) {
        self.all.iter_mut().for_each(|(_, v)| *v = false);
    }

    fn update_internal(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.pointer.primary_down()) {
            if self.state.is_some() {
                self.clear(); // clear all `Drag` state if we're no longer dragging
            }
            return;
        }

        let Some(cur_pos) = ctx.pointer_latest_pos() else {
            return;
        };

        let state = self.state.get_or_insert_with(|| DragState::new(cur_pos));

        let Some(dragged_id) = ctx.dragged_id() else {
            return;
        };

        if !self.all.contains_key(&dragged_id) {
            return;
        }

        let dx = (state.drag_start_position.x - cur_pos.x).abs();
        let dy = (state.drag_start_position.y - cur_pos.y).abs();

        let cur_direction = if dx > dy {
            DragDirection::Horizontal
        } else {
            DragDirection::Vertical
        };

        if state.drag_direction != cur_direction {
            state.drag_direction = cur_direction;
        }

        let cache = match cur_direction {
            DragDirection::Horizontal => &mut self.horizontal,
            DragDirection::Vertical => &mut self.vertical,
        };

        let Some(highest_priority) = cache.last() else {
            return;
        };

        if dragged_id != highest_priority.id {
            ctx.set_dragged_id(highest_priority.id);
        }
    }

    fn clear(&mut self) {
        *self = Drag::default()
    }
}

struct DragState {
    drag_start_position: egui::Pos2,
    drag_direction: DragDirection,
}

impl DragState {
    fn new(drag_start_position: Pos2) -> Self {
        Self {
            drag_start_position,
            drag_direction: DragDirection::Horizontal,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
struct DragId {
    id: Id,
    id_raw: u64,
    order_id: u64,
    priority: DragPriority,
}

impl Borrow<u64> for DragId {
    fn borrow(&self) -> &u64 {
        &self.id_raw
    }
}

impl Ord for DragId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| self.order_id.cmp(&other.order_id)) // if priorities are equal, most recently added should be prioritized first
    }
}

impl PartialOrd for DragId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum DragDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub enum DragPriority {
    Low,
    Medium,
    Highest,
}

pub enum IdType<'a, R> {
    ScrollArea(&'a ScrollAreaOutput<R>),
    CustomDragId(egui::Id),
}
