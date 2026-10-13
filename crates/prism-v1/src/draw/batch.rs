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
        self.commands.sort_by_key(sort_key);
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

fn sort_key(cmd: &DrawCommand) -> u8 {
    match cmd {
        DrawCommand::Shape { layer, .. }
        | DrawCommand::Mesh { layer, .. }
        | DrawCommand::Texture { layer, .. }
        | DrawCommand::Text { layer, .. }
        | DrawCommand::Material { layer, .. }
        | DrawCommand::NinePatch { layer, .. } => *layer,
    }
}
