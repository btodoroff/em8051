
use std::sync::{mpsc,Arc};
use std::thread;
use std::time::Duration;


mod em8051 {
    use std::sync::{Arc,Mutex,mpsc};
    use std::ops::{Index,IndexMut};
    use std::fmt;

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
    pub struct EmMemory {
	mem: Vec<u8>,
    }

    impl EmMemory {
	pub fn new(s: usize) -> EmMemory {
	    EmMemory {
		mem: Vec::with_capacity(s),
	    }
	}
    }
    impl Index<u16> for EmMemory {
	type Output = u8;
	fn index(&self, addr: u16) -> &Self::Output {
	    if addr as usize > self.mem.capacity() {
		&self.mem[addr as usize % self.mem.capacity()]
	    }
	    else {		
		&self.mem[addr as usize]
	    }
	}
    }
    impl IndexMut<u16> for EmMemory {
	fn index_mut(&mut self, addr: u16) -> &mut Self::Output {
	    if addr as usize > self.mem.capacity() {
		let cap = self.mem.capacity();
		&mut self.mem[addr as usize & cap]
	    } else {
		&mut self.mem[addr as usize]
	    }
	}
    }
    impl Index<u8> for EmMemory {
	type Output = u8;
	fn index(&self, addr: u8) -> &Self::Output {
	    if addr as usize > self.mem.capacity() {
		&self.mem[addr as usize % self.mem.capacity()]
	    }
	    else {		
		&self.mem[addr as usize]
	    }
	}
    }
    impl IndexMut<u8> for EmMemory {
	fn index_mut(&mut self, addr: u8) -> &mut Self::Output {
	    if addr as usize > self.mem.capacity() {
		let cap = self.mem.capacity();
		&mut self.mem[addr as usize & cap]
	    } else {
		&mut self.mem[addr as usize]
	    }
	}
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
	pub reg_bank: u8,
        pub cpu_regfile: EmMemory,
        pub pgm_memory: [u8; 16*1024],
        data_memory: [u8; 16*1024],
        int_memory: InternalMemory, // [u8; 256],
        pub clock_cycle: u64,
        break_clock_cycle: u64,
    }

    impl State {
	fn carry(&self) -> u8 {
	    if (self.int_memory[SfrAddr::PSW] & 0b10000000) != 0 {
		1
	    } else {
		0
	    }
	}
	fn aux_carry(&self) -> u8 {
	    if (self.int_memory[SfrAddr::PSW] & 0b01000000) != 0 {
		1
	    } else {
		0
	    }
	}
	fn write_carry(&mut self, bit: u8) {
	    let cval = self.int_memory[SfrAddr::PSW];
	    if bit == 0 {
		self.int_memory[SfrAddr::PSW] = cval & 0b10000000;
	    } else {
		self.int_memory[SfrAddr::PSW] = cval | 0b10000000;
	    }
	}	
	fn read_bit(&self, bit_addr: u8) -> u8 {
	    let bit_num:u8 = bit_addr & 0x07;
	    let mut byte_addr:u8 = bit_addr & 0xf8;
	    if byte_addr < 0x80 {
		byte_addr = 0x20 + byte_addr >> 5;
	    }
	    if self.int_memory[byte_addr] >> bit_num & 0x01 != 0 {
		1
	    } else {
		0
	    }
	}
    }

    impl fmt::Display for State {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            // Customize so only `x` and `y` are denoted.
	    write!(f, "CPU State\n").unwrap();
	    write!(f, "  run: {} @ PC:{}\n", self.run, self.pc).unwrap();
	    write!(f, "  {} cycles w/break at {}", self.clock_cycle, self.break_clock_cycle)
	}
    }

    pub fn new() -> Arc<Mutex<State>> {
        let state = Arc::new(Mutex::new(State {
            run: false,
            pc: 0,
	    reg_bank: 0,
	    cpu_regfile: EmMemory::new(32),
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
			    use ControlMsg::*;
                            match ctl_msg {
                                Halt => {
                                    let mut state = state_mutex.lock().unwrap();
                                    state.run = false;
                                }
                                Reset => {
                                     let mut state = state_mutex.lock().unwrap();
                                    reset(&mut state);
                                },
                                PowerOn => {
                                    let mut state = state_mutex.lock().unwrap();
                                    reset(&mut state);
                                    state.run = true;
                                },
                                Continue => {
                                    let mut state = state_mutex.lock().unwrap();
                                    state.run = true;
                                },
                                ContinueFor(clocks) => {
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
	use SfrAddr::*;
        if instruction == 0x00 { // NOP
	    state.pc+=1;
	} else
	if (instruction & 0b00011111)==0b00010001 { // ACALL addr11
            state.pc+=2;
            let sp_value = state.int_memory[SP];
            state.int_memory[SP] += 1;
            state.int_memory[sp_value+1]=(state.pc & 0x00ff) as u8;
            state.int_memory[SP] += 1;
            state.int_memory[sp_value+2]=(state.pc >> 8) as u8;
            state.pc = ((instruction & 0b11100000) as u16) << 8;
            state.pc += state.pgm_memory[(state.pc-1) as usize] as u16;
        } else
            if (instruction & 0b11111000)==0b00101000 { // ADD A,Rn
		state.int_memory[ACC] += state.cpu_regfile[instruction & 0b00000111 + state.reg_bank*8];
		state.pc+=1;
            } else
            if instruction == 0b00100101 { // ADD A,direct
		state.int_memory[ACC] += state.int_memory[state.int_memory[state.pc]];
		state.pc += 2;
            } else
	    if (instruction & 0b11111110) == 0b00100110 { // ADD A,@Ri
		state.int_memory[ACC] += state.cpu_regfile[instruction & 0x00000001 + state.reg_bank*8];
		state.pc += 1;
	    } else
	    if instruction == 0b00100100 { // ADD A,#data
		state.pc += 1;
		state.int_memory[ACC] += state.pgm_memory[state.pc as usize];
	    } else
	    if (instruction & 0b11111000) == 0b00111000 { // ADDC A,Rn
		state.int_memory[ACC] += state.cpu_regfile[instruction & 0b00000111 + state.reg_bank*8] as u8 + state.carry();
		state.pc+=1;
            } else
            if instruction == 0b00110101 { // ADDC A,direct
		state.int_memory[ACC] += state.int_memory[state.int_memory[state.pc]] + state.carry();
		state.pc += 2;
            } else
	    if (instruction & 0b11111110) == 0b00110110 { // ADDC A,@Ri
		state.int_memory[ACC] += state.cpu_regfile[instruction & 0x00000001 + state.reg_bank*8] as u8 + state.carry();
		state.pc += 1;
	    } else
	    if instruction == 0b00110100 { // ADDC A,#data
		state.pc += 1;
		state.int_memory[ACC] += state.pgm_memory[state.pc as usize]+state.carry();
		state.pc += 1;
	    } else 
	    if (instruction & 0b00011111) == 0b00000001 { // AJMP addr11
		let addr:u16 = ((((state.pgm_memory[state.pc as usize]&0b11100000)>>5) as u16)<<8)+(state.pgm_memory[(state.pc+1) as usize] as u16);
		state.pc += 2;
		state.pc = (state.pc & 0x02ff) + addr;
	    } else
	    if (instruction & 0b11111000) == 0b01011000 { // ANL A,Rn
		state.int_memory[ACC] = state.int_memory[ACC] & state.cpu_regfile[instruction & 0x07 + state.reg_bank*8];
		state.pc += 1;
	    } else
	    if instruction == 0b01010101 { // ANL A,direct
		state.pc += 1;
		state.int_memory[ACC] = state.int_memory[ACC] & state.pgm_memory[state.pc as usize];
		state.pc += 1;
	    } else
	    if (instruction & 0b11111110) == 0b01010110 { // ANL A,@Ri
		state.int_memory[ACC] = state.int_memory[ACC] & state.cpu_regfile[instruction & 0x00000001 + state.reg_bank*8];
		state.pc+=1;
	    } else
	    if instruction == 0b01010100 { // ANL A,#data
		state.pc += 1;
		state.int_memory[ACC] = state.pgm_memory[state.pc as usize]+state.carry();
		state.pc += 1;
	    } else
	    if instruction == 0b01010010 { // ANL direct,A
		state.pc += 1;
		state.int_memory[ACC] = state.pgm_memory[state.pc as usize] & state.int_memory[ACC];
		state.pc += 1;
	    } else
	    if instruction == 0b10000010 { // ANL C,bit
		state.pc += 1;
		state.write_carry(state.read_bit(state.pgm_memory[state.pc as usize]));
		state.pc += 1;
	    } else
	    if instruction == 0b10110010 { // ANL C,/bit
		state.pc += 1;
		if state.read_bit(state.pgm_memory[state.pc as usize]) == 0 {
		    state.write_carry(1);
		} else {
		    state.write_carry(0);
		}
		state.pc += 1;
	    } else
	    if instruction == 0b10110101 { // CJNE A,direct,rel
		state.pc += 1;
		let dir_addr = state.pgm_memory[pc];
		state.pc += 1;
		let rel_addr = state.pgm_memory[pc] as i8;
		state.pc += 1;
		if state.int_memory[ACC] != state.int_memory[dir_addr] {
		    state.pc += rel_addr;
		}
		if state.int_memory[ACC] < state.int_memory[dir_addr] {
		    state.write_carry(1);
		} else {
		    state.write_carry(0);
		}
	    } else
            {};
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
    println!("{:}", state_mutex.lock().unwrap());
}
