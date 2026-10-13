#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const WHITE: Self = Self::new(255, 255, 255, 255);
    pub const BLACK: Self = Self::new(0, 0, 0, 255);
    pub const RED: Self = Self::new(255, 0, 0, 255);
    pub const GREEN: Self = Self::new(0, 255, 0, 255);
    pub const BLUE: Self = Self::new(0, 0, 255, 255);
    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const INDIANRED: Self = Self::new(205, 92, 92, 255);
    pub const LIGHTCORAL: Self = Self::new(240, 128, 128, 255);
    pub const SALMON: Self = Self::new(250, 128, 114, 255);
    pub const DARKSALMON: Self = Self::new(233, 150, 122, 255);
    pub const LIGHTSALMON: Self = Self::new(255, 160, 122, 255);
    pub const CRIMSON: Self = Self::new(220, 20, 60, 255);
    pub const FIREBRICK: Self = Self::new(178, 34, 34, 255);
    pub const DARKRED: Self = Self::new(139, 0, 0, 255);
    pub const PINK: Self = Self::new(255, 192, 203, 255);
    pub const LIGHTPINK: Self = Self::new(255, 182, 193, 255);
    pub const HOTPINK: Self = Self::new(255, 105, 180, 255);
    pub const DEEPPINK: Self = Self::new(255, 20, 147, 255);
    pub const MEDIUMVIOLETRED: Self = Self::new(199, 21, 133, 255);
    pub const PALEVIOLETRED: Self = Self::new(219, 112, 147, 255);
    pub const CORAL: Self = Self::new(255, 127, 80, 255);
    pub const TOMATO: Self = Self::new(255, 99, 71, 255);
    pub const ORANGERED: Self = Self::new(255, 69, 0, 255);
    pub const DARKORANGE: Self = Self::new(255, 140, 0, 255);
    pub const ORANGE: Self = Self::new(255, 165, 0, 255);
    pub const GOLD: Self = Self::new(255, 215, 0, 255);
    pub const YELLOW: Self = Self::new(255, 255, 0, 255);
    pub const LIGHTYELLOW: Self = Self::new(255, 255, 224, 255);
    pub const LEMONCHIFFON: Self = Self::new(255, 250, 205, 255);
    pub const LIGHTGOLDENRODYELLOW: Self = Self::new(250, 250, 210, 255);
    pub const PAPAYAWHIP: Self = Self::new(255, 239, 213, 255);
    pub const MOCCASIN: Self = Self::new(255, 228, 181, 255);
    pub const PEACHPUFF: Self = Self::new(255, 218, 185, 255);
    pub const PALEGOLDENROD: Self = Self::new(238, 232, 170, 255);
    pub const KHAKI: Self = Self::new(240, 230, 140, 255);
    pub const DARKKHAKI: Self = Self::new(189, 183, 107, 255);
    pub const LAVENDER: Self = Self::new(230, 230, 250, 255);
    pub const THISTLE: Self = Self::new(216, 191, 216, 255);
    pub const PLUM: Self = Self::new(221, 160, 221, 255);
    pub const VIOLET: Self = Self::new(238, 130, 238, 255);
    pub const ORCHID: Self = Self::new(218, 112, 214, 255);
    pub const FUCHSIA: Self = Self::new(255, 0, 255, 255);
    pub const MAGENTA: Self = Self::new(255, 0, 255, 255);
    pub const MEDIUMORCHID: Self = Self::new(186, 85, 211, 255);
    pub const MEDIUMPURPLE: Self = Self::new(147, 112, 219, 255);
    pub const REBECCAPURPLE: Self = Self::new(102, 51, 153, 255);
    pub const BLUEVIOLET: Self = Self::new(138, 43, 226, 255);
    pub const DARKVIOLET: Self = Self::new(148, 0, 211, 255);
    pub const DARKORCHID: Self = Self::new(153, 50, 204, 255);
    pub const DARKMAGENTA: Self = Self::new(139, 0, 139, 255);
    pub const PURPLE: Self = Self::new(128, 0, 128, 255);
    pub const DARKPURPLE: Self = Self::new(112, 31, 126, 255);
    pub const INDIGO: Self = Self::new(75, 0, 130, 255);
    pub const SLATEBLUE: Self = Self::new(106, 90, 205, 255);
    pub const DARKSLATEBLUE: Self = Self::new(72, 61, 139, 255);
    pub const MEDIUMSLATEBLUE: Self = Self::new(123, 104, 238, 255);
    pub const GREENYELLOW: Self = Self::new(173, 255, 47, 255);
    pub const CHARTREUSE: Self = Self::new(127, 255, 0, 255);
    pub const LAWNGREEN: Self = Self::new(124, 252, 0, 255);
    pub const LIME: Self = Self::new(0, 255, 0, 255);
    pub const LIMEGREEN: Self = Self::new(50, 205, 50, 255);
    pub const PALEGREEN: Self = Self::new(152, 251, 152, 255);
    pub const LIGHTGREEN: Self = Self::new(144, 238, 144, 255);
    pub const MEDIUMSPRINGGREEN: Self = Self::new(0, 250, 154, 255);
    pub const SPRINGGREEN: Self = Self::new(0, 255, 127, 255);
    pub const MEDIUMSEAGREEN: Self = Self::new(60, 179, 113, 255);
    pub const SEAGREEN: Self = Self::new(46, 139, 87, 255);
    pub const FORESTGREEN: Self = Self::new(34, 139, 34, 255);
    pub const DARKGREEN: Self = Self::new(0, 100, 0, 255);
    pub const YELLOWGREEN: Self = Self::new(154, 205, 50, 255);
    pub const OLIVEDRAB: Self = Self::new(107, 142, 35, 255);
    pub const OLIVE: Self = Self::new(128, 128, 0, 255);
    pub const DARKOLIVEGREEN: Self = Self::new(85, 107, 47, 255);
    pub const MEDIUMAQUAMARINE: Self = Self::new(102, 205, 170, 255);
    pub const DARKSEAGREEN: Self = Self::new(143, 188, 139, 255);
    pub const LIGHTSEAGREEN: Self = Self::new(32, 178, 170, 255);
    pub const DARKCYAN: Self = Self::new(0, 139, 139, 255);
    pub const TEAL: Self = Self::new(0, 128, 128, 255);
    pub const AQUA: Self = Self::new(0, 255, 255, 255);
    pub const CYAN: Self = Self::new(0, 255, 255, 255);
    pub const LIGHTCYAN: Self = Self::new(224, 255, 255, 255);
    pub const PALETURQUOISE: Self = Self::new(175, 238, 238, 255);
    pub const AQUAMARINE: Self = Self::new(127, 255, 212, 255);
    pub const TURQUOISE: Self = Self::new(64, 224, 208, 255);
    pub const MEDIUMTURQUOISE: Self = Self::new(72, 209, 204, 255);
    pub const DARKTURQUOISE: Self = Self::new(0, 206, 209, 255);
    pub const CADETBLUE: Self = Self::new(95, 158, 160, 255);
    pub const STEELBLUE: Self = Self::new(70, 130, 180, 255);
    pub const LIGHTSTEELBLUE: Self = Self::new(176, 196, 222, 255);
    pub const POWDERBLUE: Self = Self::new(176, 224, 230, 255);
    pub const LIGHTBLUE: Self = Self::new(173, 216, 230, 255);
    pub const SKYBLUE: Self = Self::new(135, 206, 235, 255);
    pub const LIGHTSKYBLUE: Self = Self::new(135, 206, 250, 255);
    pub const DEEPSKYBLUE: Self = Self::new(0, 191, 255, 255);
    pub const DODGERBLUE: Self = Self::new(30, 144, 255, 255);
    pub const CORNFLOWERBLUE: Self = Self::new(100, 149, 237, 255);
    pub const ROYALBLUE: Self = Self::new(65, 105, 225, 255);
    pub const MEDIUMBLUE: Self = Self::new(0, 0, 205, 255);
    pub const DARKBLUE: Self = Self::new(0, 0, 139, 255);
    pub const NAVY: Self = Self::new(0, 0, 128, 255);
    pub const MIDNIGHTBLUE: Self = Self::new(25, 25, 112, 255);
    pub const CORNSILK: Self = Self::new(255, 248, 220, 255);
    pub const BLANCHEDALMOND: Self = Self::new(255, 235, 205, 255);
    pub const BISQUE: Self = Self::new(255, 228, 196, 255);
    pub const NAVAJOWHITE: Self = Self::new(255, 222, 173, 255);
    pub const WHEAT: Self = Self::new(245, 222, 179, 255);
    pub const BURLYWOOD: Self = Self::new(222, 184, 135, 255);
    pub const TAN: Self = Self::new(210, 180, 140, 255);
    pub const ROSYBROWN: Self = Self::new(188, 143, 143, 255);
    pub const SANDYBROWN: Self = Self::new(244, 164, 96, 255);
    pub const GOLDENROD: Self = Self::new(218, 165, 32, 255);
    pub const DARKGOLDENROD: Self = Self::new(184, 134, 11, 255);
    pub const PERU: Self = Self::new(205, 133, 63, 255);
    pub const CHOCOLATE: Self = Self::new(210, 105, 30, 255);
    pub const SADDLEBROWN: Self = Self::new(139, 69, 19, 255);
    pub const SIENNA: Self = Self::new(160, 82, 45, 255);
    pub const BROWN: Self = Self::new(165, 42, 42, 255);
    pub const DARKBROWN: Self = Self::new(76, 63, 47, 255);
    pub const MAROON: Self = Self::new(128, 0, 0, 255);
    pub const SNOW: Self = Self::new(255, 250, 250, 255);
    pub const HONEYDEW: Self = Self::new(240, 255, 240, 255);
    pub const MINTCREAM: Self = Self::new(245, 255, 250, 255);
    pub const AZURE: Self = Self::new(240, 255, 255, 255);
    pub const ALICEBLUE: Self = Self::new(240, 248, 255, 255);
    pub const GHOSTWHITE: Self = Self::new(248, 248, 255, 255);
    pub const WHITESMOKE: Self = Self::new(245, 245, 245, 255);
    pub const SEASHELL: Self = Self::new(255, 245, 238, 255);
    pub const BEIGE: Self = Self::new(245, 245, 220, 255);
    pub const OLDLACE: Self = Self::new(253, 245, 230, 255);
    pub const FLORALWHITE: Self = Self::new(255, 250, 240, 255);
    pub const IVORY: Self = Self::new(255, 255, 240, 255);
    pub const ANTIQUEWHITE: Self = Self::new(250, 235, 215, 255);
    pub const LINEN: Self = Self::new(250, 240, 230, 255);
    pub const LAVENDERBLUSH: Self = Self::new(255, 240, 245, 255);
    pub const MISTYROSE: Self = Self::new(255, 228, 225, 255);
    pub const GAINSBORO: Self = Self::new(220, 220, 220, 255);
    pub const LIGHTGRAY: Self = Self::new(211, 211, 211, 255);
    pub const SILVER: Self = Self::new(192, 192, 192, 255);
    pub const DARKGRAY: Self = Self::new(169, 169, 169, 255);
    pub const GRAY: Self = Self::new(128, 128, 128, 255);
    pub const DIMGRAY: Self = Self::new(105, 105, 105, 255);
    pub const LIGHTSLATEGRAY: Self = Self::new(119, 136, 153, 255);
    pub const SLATEGRAY: Self = Self::new(112, 128, 144, 255);
    pub const DARKSLATEGRAY: Self = Self::new(47, 79, 79, 255);
    pub const BLANK: Self = Self::new(0, 0, 0, 0);
    pub const RAYWHITE: Self = Self::new(245, 245, 245, 255);

    pub fn alpha(self, a: f32) -> Self {
        Self {
            a: (a.clamp(0.0, 1.0) * 255.0) as u8,
            ..self
        }
    }

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }
}
