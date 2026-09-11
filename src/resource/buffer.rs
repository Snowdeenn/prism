use utils::{
    arena::Arena,
    ids::{BufferId, BufferTag},
};

use crate::BufferError;
use crate::context::GpuContext;

pub struct GpuBuffer {
    pub buffer: wgpu::Buffer,
    pub size: u64,
    pub usage: wgpu::BufferUsages,
}

pub struct GpuBufferManager {
    buffers: Arena<GpuBuffer, BufferTag>,
}

impl GpuBufferManager {
    pub fn new() -> Self {
        Self {
            buffers: Arena::new(),
        }
    }

    pub fn create_buffer(
        &mut self,
        ctx: &GpuContext,
        label: Option<&str>,
        size: u64,
        usage: wgpu::BufferUsages,
    ) -> Result<BufferId, BufferError> {
        if size == 0 {
            tracing::error!("Impossible de créer un GPU Buffer de 0 octet");
            return Err(BufferError::InvalidSize { size });
        }
        let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size,
            usage,
            mapped_at_creation: false,
        });

        let id = self.buffers.insert(GpuBuffer {
            buffer,
            size,
            usage,
        });

        tracing::debug!(id = %id, label = ?label, size, "Buffer GPU créé");
        Ok(id)
    }

    pub fn write_buffer(
        &self,
        ctx: &GpuContext,
        id: BufferId,
        data: &[u8],
    ) -> Result<(), BufferError> {
        let buffer = self.buffers.get(id).ok_or_else(|| {
            tracing::error!(id = %id, "Tentative d'écriture sur un buffer inexistant");
            BufferError::NotFound { id }
        })?;

        if !buffer.usage.contains(wgpu::BufferUsages::COPY_DST) {
            tracing::error!(id = %id, "Écriture impossible : le buffer n'a pas le flag COPY_DST");
            return Err(BufferError::MissingCopyDstUsage { id });
        }

        let data_len = data.len() as u64;
        if data_len > buffer.size {
            tracing::error!(
                id = %id,
                required = data_len,
                available = buffer.size,
                "Débordement de mémoire évité lors de l'écriture sur le buffer"
            );
            return Err(BufferError::Overflow {
                id,
                required: data_len,
                available: buffer.size,
            });
        }

        ctx.queue.write_buffer(&buffer.buffer, 0, data);
        tracing::trace!(id = %id, bytes_written = data_len, "Mise à jour du buffer GPU");
        Ok(())
    }

    pub fn get(&self, id: BufferId) -> Option<&GpuBuffer> {
        self.buffers.get(id)
    }

    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut GpuBuffer> {
        self.buffers.get_mut(id)
    }

    pub fn remove(&mut self, id: BufferId) -> Result<GpuBuffer, BufferError> {
        if let Some(buffer) = self.buffers.remove(id) {
            tracing::debug!(id = %id, "Buffer GPU supprimé de l'Arena");
            Ok(buffer)
        } else {
            tracing::warn!(id = %id, "Tentative de suppression d'un buffer inexistant");
            Err(BufferError::NotFound { id })
        }
    }
}
