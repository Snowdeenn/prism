use std::{fmt, hash::Hash, marker::PhantomData};

/// Identifiant générationnel fortement typé.
pub struct Id<Tag> {
    pub index: usize,
    pub generation: u32,
    marker: PhantomData<Tag>,
}

impl<Tag> Copy for Id<Tag> {}
impl<Tag> Clone for Id<Tag> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<Tag> PartialEq for Id<Tag> {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.generation == other.generation
    }
}
impl<Tag> Eq for Id<Tag> {}
impl<Tag> Hash for Id<Tag> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.generation.hash(state);
    }
}
impl<Tag> fmt::Debug for Id<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Id")
            .field("index", &self.index)
            .field("generation", &self.generation)
            .finish()
    }
}
impl<Tag> fmt::Display for Id<Tag> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "idx: {} gen: {}", self.index, self.generation)
    }
}

struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// Arène générationnelle empêchant la réutilisation d'identifiants périmés.
pub struct Arena<T, Tag = T> {
    slots: Vec<Slot<T>>,
    free: Vec<usize>,
    marker: PhantomData<Tag>,
}

impl<T, Tag> Arena<T, Tag> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            marker: PhantomData,
        }
    }

    pub fn insert(&mut self, value: T) -> Id<Tag> {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index];
            slot.generation = slot.generation.wrapping_add(1);
            slot.value = Some(value);
            Id {
                index,
                generation: slot.generation,
                marker: PhantomData,
            }
        } else {
            let index = self.slots.len();
            self.slots.push(Slot {
                generation: 0,
                value: Some(value),
            });
            Id {
                index,
                generation: 0,
                marker: PhantomData,
            }
        }
    }

    pub fn get(&self, id: Id<Tag>) -> Option<&T> {
        self.slots
            .get(id.index)
            .filter(|slot| slot.generation == id.generation)?
            .value
            .as_ref()
    }

    pub fn get_mut(&mut self, id: Id<Tag>) -> Option<&mut T> {
        self.slots
            .get_mut(id.index)
            .filter(|slot| slot.generation == id.generation)?
            .value
            .as_mut()
    }

    pub fn remove(&mut self, id: Id<Tag>) -> Option<T> {
        let slot = self.slots.get_mut(id.index)?;
        if slot.generation != id.generation {
            return None;
        }
        let value = slot.value.take()?;
        self.free.push(id.index);
        Some(value)
    }
}

impl<T, Tag> Default for Arena<T, Tag> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Arena;

    #[test]
    fn stale_ids_are_rejected_after_reuse() {
        let mut arena = Arena::<i32>::new();
        let old = arena.insert(1);
        assert_eq!(arena.remove(old), Some(1));
        let new = arena.insert(2);
        assert_ne!(old.generation, new.generation);
        assert_eq!(arena.get(old), None);
        assert_eq!(arena.get(new), Some(&2));
    }
}
