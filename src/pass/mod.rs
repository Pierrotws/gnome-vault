// pass/

// Pure pass domain and persistence.

// This layer should know:

// what a pass node is
// what an entry looks like
// how to load/save entries

// It should not know:

// which node is selected in the UI
// whether the current entry is dirty
// whether the save button should be enabled

pub mod model;
pub mod store;
