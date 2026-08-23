//! Recursive variant of `portal_rooms`.
//!
//! The implementation stays shared so fixes to the base six-face portal cube
//! automatically apply here. `portal_rooms::recursive_demo_enabled` selects
//! the nested Ember-room setup from this binary's executable name.

#[path = "portal_rooms.rs"]
mod portal_rooms;

fn main() {
    portal_rooms::main();
}
