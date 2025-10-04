use crate::console::*;



pub enum ConsoleCommand
{
    Home,
    Help,
    ShowLogList,
    SelectLog,
    ClearScreen,
    BackupLog,
}

impl ConsoleMonitor {
    pub fn command_func_home(&mut self) {
        // Go to Main Menu
    }

    pub fn command_fun_help(&mut self) {
        // Show Help
    }
    
    pub fn command_func_show_log_list(&mut self) {
        // Show Log List
    }

    pub fn command_func_select_log(&mut self) {
        // Select Log
    }

    pub fn command_func_clear_screen(&mut self) {
        // Clear Screen
    }

    pub fn command_func_backup_log(&mut self) {
        // Backup Log
    }
}       