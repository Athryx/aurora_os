use core::hash::BuildHasherDefault;

use hashbrown::HashMap as HashbrownMap;
use rustc_hash::FxHasher;

pub type HashMap<K, V> = HashbrownMap<K, V, BuildHasherDefault<FxHasher>>;