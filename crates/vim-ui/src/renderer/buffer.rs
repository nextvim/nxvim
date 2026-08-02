use crate::types::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub symbol: char,
    pub fg: Color,
    pub bg: Color,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            symbol: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }
}

pub struct ScreenBuffer {
    pub cells: Vec<Cell>,
    pub width: u16,
    pub height: u16,
}

impl ScreenBuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            cells: vec![Cell::default(); (width as usize) * (height as usize)],
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.cells
            .resize((width as usize) * (height as usize), Cell::default());
        self.clear();
    }

    pub fn clear(&mut self) {
        for cell in self.cells.iter_mut() {
            *cell = Cell::default();
        }
    }

    pub fn set_cell(&mut self, x: u16, y: u16, cell: Cell) {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            self.cells[idx] = cell;
        }
    }

    pub fn get_cell(&self, x: u16, y: u16) -> Option<&Cell> {
        if x < self.width && y < self.height {
            let idx = (y as usize) * (self.width as usize) + (x as usize);
            Some(&self.cells[idx])
        } else {
            None
        }
    }
}
