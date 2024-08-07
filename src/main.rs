use std::sync::{mpsc,Arc,Mutex};
use std::thread;
use std::time::Duration;

mod em8051 {
    use std::sync::{Arc,Mutex,mpsc};

    pub enum ControlMsg {
        Halt,
        PowerOn,
        Reset,
        Continue,
        ContinueFor(u64),
    }

    pub struct State {
        pub run: bool,
        cpu_regfile: [u8; 16],
        pgm_memory: [u8; 16*1024],
        data_memory: [u8; 16*1024],
        sfr_space: [u8; 128],
        pub clock_cycle: u64,
        break_clock_cycle: u64,
    }

    pub fn new() -> Arc<Mutex<State>> {
        let state = Arc::new(Mutex::new(State {
            run: false,
            cpu_regfile: [0; 16],
            pgm_memory: [0; 16*1024],
            data_memory: [0; 16*1024],
            sfr_space: [0; 128],
            clock_cycle: 0,
            break_clock_cycle: u64::MAX,
        }));
        state
    }

    pub fn startup (ctrlrx: mpsc::Receiver<ControlMsg>, state_mutex: Arc<Mutex<State>>) {
        loop {
            loop {
                match ctrlrx.try_recv() {
                    Err(_) => { break; },
                    Ok(ctl_msg) => {
                            match ctl_msg {
                                ControlMsg::Halt => {
                                    let mut state = state_mutex.lock().unwrap();
                                    state.run = false;
                                }
                                ControlMsg::Reset => {
                                    let mut state = state_mutex.lock().unwrap();
                                    reset(&mut state);
                                },
                                ControlMsg::PowerOn => {
                                    let mut state = state_mutex.lock().unwrap();
                                    reset(&mut state);
                                    state.run = true;
                                },
                                ControlMsg::Continue => {
                                    let mut state = state_mutex.lock().unwrap();
                                    state.run = true;
                                },
                                ControlMsg::ContinueFor(clocks) => {
                                    let mut state = state_mutex.lock().unwrap();
                                    state.break_clock_cycle = state.clock_cycle+clocks;
                                    state.run = true;
                                },
                            }
                    }
                }
            }

            if state_mutex.lock().unwrap().run {
                let mut state = state_mutex.lock().unwrap();
                tick(&mut state);

                if state.clock_cycle >= state.break_clock_cycle {
                    state.run = false;
                    state.break_clock_cycle = u64::MAX;
                }
            }

        }
    }

    fn tick (state: &mut State) {
        state.clock_cycle += 1;
        println!("Tick: {}",state.clock_cycle);
    }

    fn reset (state: &mut State) {
        for i in 0..128 {
            state.sfr_space[i]=0;
        }
    }

}

fn main() {
    println!("Hello, world!");
    let (tx, rx) = mpsc::channel();
    let state_mutex = em8051::new();
    let passed_mutex = Arc::clone(&state_mutex);
    thread::spawn(move || {em8051::startup(rx, passed_mutex);});
    tx.send(em8051::ControlMsg::Reset).unwrap();
    tx.send(em8051::ControlMsg::ContinueFor(10)).unwrap();
    println!("A");
    while state_mutex.lock().unwrap().clock_cycle < 10 {thread::sleep(Duration::from_millis(1));}
    println!("{}", state_mutex.lock().unwrap().clock_cycle);
}
