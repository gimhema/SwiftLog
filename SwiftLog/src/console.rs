use crate::console_command::*;


pub struct ConsoleMonitor
{
    // Add fields as necessary
    
}

impl ConsoleMonitor
{
    pub fn new() -> Self
    {
        ConsoleMonitor {
            // Initialize fields
        }
    }

    pub fn start(&self)
    {
        // Start monitoring console output

        // wait input command in loop

        // on command, call do_command_process

        self.command_loop();
    }

    pub fn command_loop(&self)
    {
        // Loop to read commands from console
        loop {

            // wait input command
            
        }
    }

    pub fn do_command_process(&mut self, command: ConsoleCommand)
    {
        // Process commands
        match command {
            ConsoleCommand::Home => self.command_func_home(),
            ConsoleCommand::Help => self.command_fun_help(),
            ConsoleCommand::ShowLogList => self.command_func_show_log_list(),
            ConsoleCommand::SelectLog => self.command_func_select_log(),
            ConsoleCommand::ClearScreen => self.command_func_clear_screen(),
            ConsoleCommand::BackupLog => self.command_func_backup_log(),
        }
    }

}


