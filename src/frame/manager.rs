use crossbeam::queue::ArrayQueue;
use std::sync::Arc;

use crate::frame::frame::Frame;

/// Gestionnaire de file d'attente *lock-free* (FIFO) pour transférer les `Frame`s
/// du thread logique (boucle de jeu) vers le thread de rendu GPU.
#[derive(Debug)]
pub struct FrameManager {
    queue: Arc<ArrayQueue<Frame>>,
}

impl Default for FrameManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameManager {
    /// Crée un nouveau `FrameManager` configuré par défaut en *double-buffering* (capacité de 2).
    pub fn new() -> Self {
        Self::with_capacity(2)
    }

    /// Crée un `FrameManager` avec une capacité personnalisée.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            queue: Arc::new(ArrayQueue::new(capacity)),
        }
    }

    /// Thread principal — envoie une frame prête au rendu.
    ///
    /// Si la file d'attente est pleine (thread de rendu à la traîne), la frame est rejetée,
    /// retournée dans le `Err(frame)` et un avertissement est consigné dans les logs.
    pub fn push(&self, frame: Frame) -> Result<(), Frame> {
        self.queue.push(frame).map_err(|rejected_frame| {
            tracing::warn!(
                "File d'attente des frames pleine : frame rejetée pour éviter le blocage"
            );
            rejected_frame
        })
    }

    /// Thread de rendu — consomme la prochaine frame disponible.
    pub fn pop(&self) -> Option<Frame> {
        self.queue.pop()
    }

    /// Clone l'Arc pour partager la file d'attente entre threads.
    pub fn handle(&self) -> Arc<ArrayQueue<Frame>> {
        Arc::clone(&self.queue)
    }

    /// Indique si la file ne contient aucune frame en attente.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Retourne le nombre de frames actuellement prêtes dans la file d'attente.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Retourne la capacité maximale de la file d'attente.
    pub fn capacity(&self) -> usize {
        self.queue.capacity()
    }

    pub fn clear(&self) {
        while self.queue.pop().is_some() {}
    }
}
