use lazy_static::lazy_static;
use std::sync::Mutex;

pub struct GlobalState {
    pub e_flag: bool,
    pub t_flag: bool,
    pub comp_flag: bool,
    pub max_cost: i32,
    pub time_to_swap: i32,
    pub time_to_clean_up: i32,
    pub default_fail_message: Option<String>,
    pub boot_time: i64,
    pub max_array_size: i32,
    pub max_buffer_size: i32,
    pub max_string_length: i32,
    pub reserved_size: i32,
    pub reserved_area: Option<Vec<u8>>,
    pub mud_lib: Option<String>,
    pub consts: [f64; 10],
}

impl GlobalState {
    fn new() -> Self {
        GlobalState {
            e_flag: false,
            t_flag: false,
            comp_flag: false,
            max_cost: 100000,
            time_to_swap: 10,
            time_to_clean_up: 10,
            default_fail_message: Some("What?\n".to_string()),
            boot_time: 0,
            max_array_size: 1000,
            max_buffer_size: 10000,
            max_string_length: 10000,
            reserved_size: 0,
            reserved_area: None,
            mud_lib: Some("mudlib".to_string()),
            consts: [0.0; 10],
        }
    }
}

lazy_static! {
    pub static ref GLOBALS: Mutex<GlobalState> = Mutex::new(GlobalState::new());
}
