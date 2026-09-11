use crate::draw::commands::DrawCommand;

#[derive(Debug)]
pub struct DrawCommandBuffer {
    commands: Vec<DrawCommand>,
}

impl DrawCommandBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            commands: Vec::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, command: DrawCommand) {
        self.commands.push(command);
    }

    pub fn sort_commands(&mut self) {
        self.commands.sort_unstable_by_key(|cmd| sort_key(cmd));
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands[..]
    }
    pub fn commands_mut(&mut self) -> &mut [DrawCommand] {
        &mut self.commands[..]
    }
}

fn sort_key(cmd: &DrawCommand) -> u64 {
    match cmd {
        DrawCommand::Shape { layer, blend, .. } => (*layer as u64) << 56 | (*blend as u64) << 48,
        DrawCommand::Mesh { layer, blend, .. } => (*layer as u64) << 56 | (*blend as u64) << 48,
        DrawCommand::Texture {
            layer, blend, id, ..
        } => (*layer as u64) << 56 | (*blend as u64) << 48 | (id.index as u64),
        DrawCommand::Text { layer, .. } => (*layer as u64) << 56 | (1u64 << 32),
        DrawCommand::Material {
            layer,
            material_id,
            texture_id,
            blend,
            ..
        } => {
            (*layer as u64) << 56
                | (*blend as u64) << 48
                | (material_id.index as u64) << 16
                | texture_id.map(|t| t.index as u64).unwrap_or(0)
        }
        DrawCommand::NinePatch {
            layer, blend, id, ..
        } => (*layer as u64) << 56 | (*blend as u64) << 48 | (id.index as u64),
    }
}
