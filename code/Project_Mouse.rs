//! rtic_bare7.rs
//!
//! HAL OutputPin abstractions
//!
//! What it covers:
//! - using embedded hal, and the OutputPin abstraction

#![no_main]
#![no_std]
use embedded_hal::spi::MODE_3;
use panic_rtt_target as _;
use rtt_target as _;
use cortex_m::{asm::delay, peripheral::DWT};
use embedded_hal::digital::v2::{OutputPin, ToggleableOutputPin};
use stm32f4xx_hal::{
    gpio,
    otg_fs::{UsbBus, UsbBusType, USB},
    prelude::*,
};
use usb_device::{bus::UsbBusAllocator, prelude::*};
use usbd_hid::{
    descriptor::{generator_prelude::*, MouseReport},
    hid_class::HIDClass
};
use app::{DwtDelay, pmw3389::{self, Register}, pmw3389e};
use usbd_hid::descriptor::SerializedDescriptor;
use usbd_hid::descriptor::AsInputReport;
use usbd_hid::descriptor::gen_hid_descriptor;
#[gen_hid_descriptor(
    (collection = APPLICATION, usage_page = GENERIC_DESKTOP, usage = MOUSE) = {
        (collection = PHYSICAL, usage = POINTER) = {
            (usage_page = BUTTON, usage_min = 0x01, usage_max = 0x05) = {
                #[packed_bits 5] #[item_settings data,variable,absolute] buttons=input;
            };
            (usage_page = GENERIC_DESKTOP,) = {
                (usage = X,) = {
                    #[item_settings data,variable,relative] x=input;
                };
                (usage = Y,) = {
                    #[item_settings data,variable,relative] y=input;
                };
                (usage = WHEEL,) = {
                    #[item_settings data,variable,relative] wheel=input;
                };
            };
        };
    }
)]
pub struct PMouseReport {
    pub buttons: u8,
    pub x: i8,
    pub y: i8,
    pub wheel: i8, // Scroll down (negative) or up (positive) this many units
}

use rtic::cyccnt::{Instant, U32Ext as _};
use stm32f4xx_hal::{
    dwt::Dwt,
    gpio::Speed,
    gpio::{
        gpiob::{PB10, PB12},
        gpioc::{PC2, PC3},
        Alternate, Output, PushPull,
    },
    prelude::*,
    rcc::Clocks,
    spi::Spi,
    stm32,
};

type PMW3389T = pmw3389::Pmw3389<
    Spi<
        stm32f4xx_hal::stm32::SPI2,
        (
            PB10<Alternate<stm32f4xx_hal::gpio::AF5>>,
            PC2<Alternate<stm32f4xx_hal::gpio::AF5>>,
            PC3<Alternate<stm32f4xx_hal::gpio::AF5>>,
        ),
    >,
    PB12<Output<PushPull>>,
>;
use rtt_target::{rprintln, rtt_init_print};
use stm32f4xx_hal::nb::block;
//use rtic_core::Mutex;

use stm32f4xx_hal::{
    gpio::{gpioa::PA9},
    gpio::{gpioa::PA0, gpioa::PA1, gpioa::PA2, gpioa::PA3, gpioa::PA4, gpioa::PA5, gpioa::PA6, gpioa::PA10, gpioa::PA15, Input, PullUp},
    //gpio::{gpioc::PC10},
    //gpio::{gpioc::PC12},
    prelude::*,
};

const OFFSET: u32 = 1_000_000;

#[rtic::app(device = stm32f4xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        // late resources
        hid: HIDClass<'static, UsbBusType>,
        usb_dev: UsbDevice<'static, UsbBusType>,
        pmw3389: PMW3389T,
        led: PA9<Output<PushPull>>,
        r_click: PA1<Input<PullUp>>,
        l_click: PA0<Input<PullUp>>,
        w_click: PA6<Input<PullUp>>,
        M1_click: PA4<Input<PullUp>>,
        M2_click: PA5<Input<PullUp>>,
        scl_plus: PA2<Input<PullUp>>,
        scl_minus: PA3<Input<PullUp>>,
        scroll_up: PA10<Input<PullUp>>,
        scroll_down: PA15<Input<PullUp>>,
        Scaler: f32, //rtic::Mutex,
        Counter: u8,
        Scale_modify: bool,
    }
    
    #[init(schedule = [toggle_speed])]
    fn init(cx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<UsbBusAllocator<UsbBusType>> = None;
        static mut EP_MEMORY: [u32; 1024] = [0; 1024];
        rtt_init_print!();
        rprintln!("init");
        let mut core = cx.core;
        core.DCB.enable_trace();
        DWT::unlock();
        core.DWT.enable_cycle_counter();

        let rcc = cx.device.RCC.constrain();

        let clocks = rcc
            .cfgr
            // .use_hse(8.mhz())
            .sysclk(48.mhz())
            .pclk1(24.mhz())
            .freeze();

        // assert!(clocks.usbclk_valid());

        let gpioa = cx.device.GPIOA.split();

        // Pull the D+ pin down to send a RESET condition to the USB bus.
        //let mut usb_dp = gpioa.pa12.into_push_pull_output();
        //usb_dp.set_low().ok();
        //delay(clocks.sysclk().0 / 100);
        //let usb_dp = usb_dp.into_floating_input();
        //cortex_m::asm::delay(100);

        //let usb_dm = gpioa.pa11;

        let usb = USB {
            usb_global: cx.device.OTG_FS_GLOBAL,
            usb_device: cx.device.OTG_FS_DEVICE,
            usb_pwrclk: cx.device.OTG_FS_PWRCLK,
            pin_dm: gpioa.pa11.into_alternate_af10(),
            pin_dp: gpioa.pa12.into_alternate_af10(),
        };
	
        USB_BUS.replace(UsbBus::new(usb, EP_MEMORY));
        let hid = HIDClass::new(USB_BUS.as_ref().unwrap(), PMouseReport::desc(), 1);


        let usb_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(0xc410, 0x0000))
            .manufacturer("Mouse company")
            .product("Mouse")
            .serial_number("Serial_Number")
            .device_class(0)
            .build();

        let gpiob = cx.device.GPIOB.split();
        let gpioc = cx.device.GPIOC.split();

        let sck = gpiob.pb10.into_alternate_af5();
        let miso = gpioc.pc2.into_alternate_af5();
        let mosi = gpioc.pc3.into_alternate_af5();
        let cs = gpiob.pb12.into_push_pull_output().set_speed(Speed::High);

        let spi = Spi::spi2(
            cx.device.SPI2,
            (sck, miso, mosi),
            MODE_3,
            stm32f4xx_hal::time::KiloHertz(2000).into(),
            clocks,
        );

        let delay = DwtDelay::new(&mut core.DWT, clocks);
        let mut pmw3389 = pmw3389::Pmw3389::new(spi, cs, delay).unwrap();
        // set in burst mode
        pmw3389.write_register(Register::MotionBurst, 0x00);
        let scaler = 1.0;
        let scale_modify = false;

        let now = cx.start;
            
        cx.schedule.toggle_speed(now + ((OFFSET)).cycles()).unwrap();
        //cx.schedule.toggle(now + ((OFFSET)).cycles()).unwrap();
        //let mut led = ;
        //led.set_low().ok();
        // pass on late resources
        init::LateResources {
            //GPIOA: device.GPIOA,
            hid,
            usb_dev,
            led: gpioa.pa9.into_push_pull_output(), //split the GPIOA into pins, choose pa5 and convert into push/pull output (this took a while to figure out)
            r_click: gpioa.pa1.into_pull_up_input(),
            l_click: gpioa.pa0.into_pull_up_input(),
            w_click: gpioa.pa6.into_pull_up_input(),
            M1_click: gpioa.pa4.into_pull_up_input(),
            M2_click: gpioa.pa5.into_pull_up_input(),
            scl_plus: gpioa.pa2.into_pull_up_input(),
            scl_minus: gpioa.pa3.into_pull_up_input(),
            scroll_up: gpioa.pa10.into_pull_up_input(),
            scroll_down: gpioa.pa15.into_pull_up_input(),
            Scaler: scaler,
            Counter: 0,
            Scale_modify: scale_modify,
            pmw3389,
            }      
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        rprintln!("idle");
        loop {
            continue;
        }
    }

    //Increase or lower frequency
    #[task(resources = [scl_minus, scl_plus, Scaler, Scale_modify], priority = 1, schedule = [toggle_speed])]
    fn toggle_speed(mut cx: toggle_speed::Context) {
        let Scale_modify = *cx.resources.Scale_modify;
            if (cx.resources.scl_plus.is_high().unwrap() && !*cx.resources.Scale_modify){
                *cx.resources.Scale_modify = true;
                cx.resources.Scaler.lock(|Scaler| {
                    *Scaler += 0.1;
                });
            }
            else{
                if cx.resources.scl_plus.is_low().unwrap() && cx.resources.scl_minus.is_low().unwrap(){
                    *cx.resources.Scale_modify = false;
                }
            }
            if (cx.resources.scl_minus.is_high().unwrap() && !*cx.resources.Scale_modify){
                *cx.resources.Scale_modify = true;
                cx.resources.Scaler.lock(|Scaler| {
                if *Scaler != 1.0 && !(*Scaler < 1.0){
                    *Scaler -= 0.1;
                }
                else{
                    *Scaler = 1.0;
                }
                });
            }
            else{
                if cx.resources.scl_plus.is_high().unwrap() && cx.resources.scl_minus.is_high().unwrap(){
                    *cx.resources.Scale_modify = false;
                }
            }
        cx.schedule.toggle_speed(cx.scheduled + ((OFFSET)).cycles()).unwrap();
    }
    
    extern "C" {
        fn EXTI0();
    }
    
    #[task(binds=OTG_FS, resources = [led, r_click, l_click, w_click, M1_click, M2_click, scroll_up, scroll_down, Scaler, hid, pmw3389, usb_dev], priority = 2)]
    fn toggle(cx: toggle::Context) {
        static mut PREV_UP: bool = false;
        static mut PREV_DOWN: bool = false;

        let myScaler = cx.resources.Scaler;
        let hid = cx.resources.hid;
        let r_click = cx.resources.r_click;
        let l_click = cx.resources.l_click;
        let w_click = cx.resources.w_click;
        let M1_click = cx.resources.M1_click;
        let M2_click = cx.resources.M2_click;
        let usb_dev = cx.resources.usb_dev;
        let scroll_up = cx.resources.scroll_up;
        let scroll_down = cx.resources.scroll_down;
        let up = scroll_up.is_high().unwrap();
        let down = scroll_down.is_high().unwrap();
        let wheel_count = calculate_scroll(up, down, *PREV_UP, *PREV_DOWN);
        *PREV_UP = up;
        *PREV_DOWN = down;
        let mut POS_X: i64 = 0;
        let mut POS_Y: i64 = 0;
        if M1_click.is_high().unwrap() {
            //if cx.resources.led.is_low().unwrap(){
              // _toggleable_generic(cx.resources.led); //Utilize the generic toggle function, toggle variable no longer needed
               //}
        }
        if M2_click.is_high().unwrap() {
            //if cx.resources.led.is_low().unwrap(){
              // _toggleable_generic(cx.resources.led); //Utilize the generic toggle function, toggle variable no longer needed
               //}
        }
        else{
            if cx.resources.led.is_low().unwrap() && r_click.is_low().unwrap() && l_click.is_low().unwrap() {
                //_toggleable_generic(cx.resources.led);
            }
        }
        
        let (x, y) = cx.resources.pmw3389.read_status().unwrap();
        //rprintln!("{} {}", x as i64, y as i64);
        POS_X = x as f32;
        POS_Y = y as f32;
        let report = PMouseReport {
            buttons: ((M1_click.is_high().unwrap() as u8) << 4
                | (M2_click.is_high().unwrap() as u8) << 3
                | (w_click.is_high().unwrap() as u8) << 2
                | (r_click.is_high().unwrap() as u8) << 1
                | (l_click.is_high().unwrap() as u8)),
            x: ((-POS_X * *myScaler) as i8)>>1,
            y: ((-POS_Y * *myScaler) as i8)>>1,
            wheel: wheel_count,
        };
        hid.push_input(&report).ok();
        
        if usb_dev.poll(&mut [hid]) {
            return;
        }
   
        
        //cx.schedule.toggle(cx.scheduled + ((*myScaler as u32 * OFFSET)).cycles()).unwrap();
    }

    extern "C" {
        fn EXTI1();
    }
    
    extern "C" {
        fn EXTI2();
    }
};

fn _toggle_generic<E>(led: &mut dyn OutputPin<Error = E>, toggle: &mut bool) {
    if *toggle {
        led.set_high().ok();
    } else {
        led.set_low().ok();
    }

    *toggle = !*toggle;
}

fn _toggleable_generic<E>(led: &mut dyn ToggleableOutputPin<Error = E>) {
    led.toggle().ok();
}

fn calculate_scroll(up: bool, down: bool, prev_up: bool, prev_down: bool) -> i8 {
    let mut wheel_count: i8 = 0;

    //only update count if values have changed
    if up != prev_up || down != prev_down {

        //check datasheet of encoder for pattern
        if prev_up == prev_down {
            if down == prev_down {
                wheel_count += 1;
            } else {
                wheel_count -= 1;
            }
        } else {
            if up == prev_up {
                wheel_count += 1;
            } else {
                wheel_count -= 1;
            }
        }
    }
    return wheel_count;
}

// 1. In this example you will use RTT.
//
//    > cargo run --example rtic_bare7
//
//    Look in the generated documentation for `set_high`/`set_low`.
//    (You created documentation for your dependencies in previous exercise
//    so you can just search (press `S`) for `OutputPin`).
//    You will find that these methods are implemented for `Output` pins.
//
//    Now change your code to use these functions instead of the low-level GPIO API.
//
//    HINTS:
//    - A GPIOx peripheral can be `split` into individual PINs Px0..Px15).
//    - A Pxy, can be turned into an `Output` by `into_push_pull_output`.
//    - You may optionally set other pin properties as well (such as `speed`).
//    - An `Output` pin provides `set_low`/`set_high`
//    - Instead of passing `GPIO` resource to the `toggle` task pass the
//      `led: PA5<Output<PushPull>>` resource instead.
//
//    Comment your code to explain the steps taken.
//
//    ** My answer here **
//    1. We had to change around in the resources, as the GPIO is no longer needed
//    this means that we have to uncomment the led variable in the resources struct
//    and insert the correct struct into lateresources.
//    2. Now we have our resources set up, so I changed the GPIO into led under the 
//    toggle function.
//
//    Confirm that your implementation correctly toggles the LED as in
//    previous exercise.
//
//    Commit your code (bare7_1)
//
// 2. Further generalizations:
//
//    Now look at the documentation for `embedded_hal::digital::v2::OutputPin`.
//
//    You see that the OutputPin trait defines `set_low`/`set_high` functions.
//    Your task is to alter the code to use the `set_low`/`set_high` API.
//
//    The function `_toggle_generic` is generic to any object that
//    implements the `OutputPin<Error = E>` trait.
//
//    Digging deeper we find the type parameter `E`, which in this case
//    is left generic (unbound).
//
//    It will be instantiated with a concrete type argument when called.
//
//    Our `PA5<Output<PushPull>>` implements `OutputPin` trait, thus
//    we can pass the `led` resource to `_toggle_generic`.
//    
//    The error type is given by the stm32f4xx-hal implementation:
//    where `core::convert::Infallible` is used to indicate
//    there are no errors to be expected (hence infallible).
//
//    Additionally, `_toggle_generic` takes a mutable reference
//    `toggle: &mut bool`, so you need to pass your `TOGGLE` variable.
//
//    As you see, `TOGGLE` holds the "state", switching between
//    `true` and `false` (to make your led blink).
//
//    Change your code into using the `_toggle_generic` function.
//    (You may rename it to `toggle_generic` if wished.)
//
//    Confirm that your implementation correctly toggles the LED as in
//    previous exercise.
//
//    Commit your code (bare7_2)
//
//    ** My answer here **
//    This was merely a question of passing the toggle variable to the generic function.
//
// 3. What about the state?
//
//    In your code `TOGGLE` holds the "state". However, the underlying
//    hardware ALSO holds the state (if the corresponding bit is set/cleared).
//
//    What if we can leverage that, and guess what we can!!!!
//
//    Look at the documentation for `embedded_hal::digital::v2::ToggleableOutputPin`,
//    and the implementation of:
//
//    fn _toggleable_generic(led: &mut dyn ToggleableOutputPin<Error = Infallible>) {
//      led.toggle().ok();
//    }
//
//    The latter does not take any state variable, instead it directly `toggle()`
//    the `ToggleableOutputPin`.
//
//    Now alter your code to leverage on the `_toggleable_generic` function.
//    (You should be able to remove the `TOGGLE` state variable altogether.)
//
//    Confirm that your implementation correctly toggles the LED as in
//    previous exercise.
//
//    ** My answer here **
//    This is merely a question of uncommenting the toggle variable, as well
//    as the calls to the functions we are not to use. Since the toggleable_generic
//    works in the same was as toggle_generic, in the sense that it generically
//    handles pins which we can read and write from, so passing the led resource
//    is enough.
//
//    Commit your code (bare7_3)
//
// 4. Discussion:
//
//    In this exercise you have gone from a very hardware specific implementation,
//    to leveraging abstractions (batteries included).
//
//    Your final code amounts to "configuration" rather than "coding".
//
//    This reduces the risk of errors (as you let the libraries do the heavy lifting).
//
//    This also improves code-re use. E.g., if you were to do something less
//    trivial then merely toggling you can do that in a generic manner,
//    breaking out functionality into "components" re-usable in other applications.
//
//    Of course the example is trivial, you don't gain much here, but the principle
//    is the same behind drivers for USART communication, USB, PMW3389 etc.
//
// 5. More details:
//    
//    Looking closer at the implementation:
//    `led: &mut dyn OutputPin<Error = E>`
//
//    You may ask what kind of mumbo jumbo is at play here.
//
//    This is the way to express that we expect a mutable reference to a trait object 
//    that implements the `OutputPin`. Since we will change the underlying object
//    (in this case an GPIOA pin 5) the reference needs to be mutable.
// 
//    Trait objects are further explained in the Rust book.
//    The `dyn` keyword indicates dynamic dispatch (through a VTABLE).
//    https://doc.rust-lang.org/std/keyword.dyn.html
//
//    Notice: the Rust compiler (rustc + LLVM) is really smart. In many cases
//    it can analyse the call chain, and conclude the exact trait object type at hand.
//    In such cases the dynamic dispatch is turned into a static dispatch
//    and the VTABLE is gone, and we have a zero-cost abstraction.
//
//    If the trait object is stored for e.g., in an array along with other
//    trait objects (of different concrete type), there is usually no telling
//    the concrete type of each element, and we will have dynamic dispatch.
//    Arguably, this is also a zero-cost abstraction, as there is no (obvious)
//    way to implement it more efficiently. Remember, zero-cost is not without cost
//    just that it is as good as it possibly gets (you can't make it better by hand).
//
//    You can also force the compiler to deduce the type at compile time, by using
//    `impl` instead of `dyn`, if you are sure you don't want the compiler to
//    "fallback" to dynamic dispatch.
//
//    You might find Rust to have long compile times. Yes you are right,
//    and this type of deep analysis done in release mode is part of the story.
//    On the other hand, the aggressive optimization allows us to code 
//    in a generic high level fashion and still have excellent performing binaries.
