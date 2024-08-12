use std::sync::{mpsc,Arc,Mutex};
use std::thread;
use std::time::Duration;


mod em8051 {
    use std::sync::{Arc,Mutex,mpsc};
    use std::ops::{Index,IndexMut};
    struct SfrData {
        address: u16,
        por_value: u8
    }

    enum SfrAddr {
        ACC = 0xE0,
        B = 0xF0,
        PSW = 0xD0,
        SP = 0x81,
        DPL = 0x82,
        DPH = 0x83,
        P0 = 0x80,
        P1 = 0x90,
        P2 = 0xA0,
        P3 = 0xB0,
        IP = 0xB8,
        IE = 0xA8,
        TMOD = 0x89,
        TCON = 0x88,
        TH0 = 0x8C,
        TL0 = 0x8A,
        TH1 = 0x8D,
        TL1 = 0x8B,
        SCON = 0x98,
        SBUF = 0x99,
        PCON = 0x87,
    }

    pub enum ControlMsg {
        Halt,
        PowerOn,
        Reset,
        Continue,
        ContinueFor(u64),
    }

    #[derive(Debug)]
    struct InternalMemory([u8;256]);
    impl Index<SfrAddr> for InternalMemory {
        type Output = u8;
        fn index(&self, sfr: SfrAddr) -> &Self::Output {
            &self.0[sfr as usize]
        }
    }
    impl IndexMut<SfrAddr> for InternalMemory {
        fn index_mut(&mut self, sfr: SfrAddr) -> &mut Self::Output {
            &mut self.0[sfr as usize]
        }
    }
    impl Index<u16> for InternalMemory {
        type Output = u8;
        fn index(&self, addr: u16) -> &Self::Output {
            &self.0[addr as usize]
        }
    }
    impl IndexMut<u16> for InternalMemory {
        fn index_mut(&mut self, addr: u16) -> &mut Self::Output {
            &mut self.0[addr as usize]
        }
    }
    impl Index<u8> for InternalMemory {
        type Output = u8;
        fn index(&self, addr: u8) -> &Self::Output {
            &self.0[addr as usize]
        }
    }
    impl IndexMut<u8> for InternalMemory {
        fn index_mut(&mut self, addr: u8) -> &mut Self::Output {
            &mut self.0[addr as usize]
        }
    }

    #[derive(Debug)]
    pub struct State {
        pub run: bool,
        pub pc: u16,
        pub cpu_regfile: [u8; 16],
        pub pgm_memory: [u8; 16*1024],
        data_memory: [u8; 16*1024],
        int_memory: InternalMemory, // [u8; 256],
        pub clock_cycle: u64,
        break_clock_cycle: u64,
    }

    pub fn new() -> Arc<Mutex<State>> {
        let state = Arc::new(Mutex::new(State {
            run: false,
            pc: 0,
            cpu_regfile: [0; 16],
            pgm_memory: [0; 16*1024],
            data_memory: [0; 16*1024],
            int_memory: InternalMemory([0; 256]),
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

                //Check clock cycle breakpoint
                if state.clock_cycle >= state.break_clock_cycle {
                    state.run = false;
                    state.break_clock_cycle = u64::MAX;
                }
            }

        }
    }

    fn tick (state: &mut State) {
        let instruction:u8 = state.pgm_memory[state.pc as usize];

        if (instruction & 0b00011111)==0b00010001 { // ACALL addr11
            state.pc+=2;
            let sp_value = state.int_memory[SfrAddr::SP];
            state.int_memory[SfrAddr::SP] += 1;
            state.int_memory[sp_value+1]=(state.pc & 0x00ff) as u8;
            state.int_memory[SfrAddr::SP] += 1;
            state.int_memory[sp_value+2]=(state.pc >> 8) as u8;
            state.pc = ((instruction & 0b11100000) as u16) << 8;
            state.pc += state.pgm_memory[(state.pc-1) as usize] as u16;
        } else
        if (instruction & 0b11111000)==0b00101000 { // ADD A,Rn
            state.int_memory[SfrAddr::ACC] += state.cpu_regfile[(instruction & 0b00000111) as usize] as u8;
            state.pc+=1;
        } else
        if instruction == 0b00100101 {
            state.int_memory[SfrAddr::ACC] += state.int_memory[state.int_memory[state.pc]];
            state.pc += 2;
        } else
        {}
        state.clock_cycle += 1;
        println!("Tick: {}",state.clock_cycle);
    }

    fn reset (state: &mut State) {
        state.run = false;
        state.pc = 0;
        for i in 0..128 {
            state.int_memory.0[i]=0;
        }
        state.int_memory[SfrAddr::SP] = 0b00000111;
        state.int_memory[SfrAddr::P0] = 0b11111111;
        state.int_memory[SfrAddr::P1] = 0b11111111;
        state.int_memory[SfrAddr::P2] = 0b11111111;
        state.int_memory[SfrAddr::P3] = 0b11111111;
        state.clock_cycle = 0;
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
    println!("{:?}", state_mutex.lock().unwrap());
}
