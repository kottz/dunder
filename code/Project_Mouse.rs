#![no_main]
#![no_std]
use embedded_hal::spi::MODE_3;
use panic_rtt_target as _;
use rtt_target as _;
use cortex_m::{peripheral::DWT};
use embedded_hal::digital::v2::{OutputPin};
use stm32f4xx_hal::{
    otg_fs::{UsbBus, UsbBusType, USB},
    prelude::*,
};
use usb_device::{bus::UsbBusAllocator, prelude::*};
use usbd_hid::{
    descriptor::{generator_prelude::*},
    hid_class::HIDClass
};
use app::{DwtDelay, pmw3389::{self, Register}};
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

use rtic::cyccnt::{U32Ext as _};
use stm32f4xx_hal::{
    gpio::Speed,
    gpio::{
        gpiob::{PB10, PB12},
        gpioc::{PC2, PC3},
        Alternate, Output, PushPull,
    },
    spi::Spi,
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

use stm32f4xx_hal::{
    gpio::{gpioa::PA7, gpioa::PA8, gpioa::PA9},
    gpio::{gpioa::PA0, gpioa::PA1, gpioa::PA2, gpioa::PA3, gpioa::PA4, gpioa::PA5, gpioa::PA6, gpioa::PA10, gpioa::PA15, Input, PullUp},
};

const OFFSET: u32 = 1_000_000;

#[rtic::app(device = stm32f4xx_hal::stm32, monotonic = rtic::cyccnt::CYCCNT, peripherals = true)]
const APP: () = {
    struct Resources {
        // late resources
        hid: HIDClass<'static, UsbBusType>,
        usb_dev: UsbDevice<'static, UsbBusType>,
        pmw3389: PMW3389T,
        led_r: PA7<Output<PushPull>>,
        led_g: PA8<Output<PushPull>>,
        led_b: PA9<Output<PushPull>>,
        r_click: PA1<Input<PullUp>>,
        l_click: PA0<Input<PullUp>>,
        w_click: PA6<Input<PullUp>>,
        M1_click: PA4<Input<PullUp>>,
        M2_click: PA5<Input<PullUp>>,
        scl_plus: PA2<Input<PullUp>>,
        scl_minus: PA3<Input<PullUp>>,
        scroll_up: PA10<Input<PullUp>>,
        scroll_down: PA15<Input<PullUp>>,
        Scaler: f32,
        Counter: u8,
        Led_Counter: u16,
        Scale_modify: bool,
    }
    
    // Initializing function
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
            .sysclk(48.mhz())
            .pclk1(24.mhz())
            .freeze();


        let gpioa = cx.device.GPIOA.split();


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
        pmw3389.write_register(Register::MotionBurst, 0x00);
        let scaler = 1.0;
        let scale_modify = false;

        let now = cx.start;
            
        cx.schedule.toggle_speed(now + ((OFFSET)).cycles()).unwrap();

        // pass on late resources
        init::LateResources {
            hid,
            usb_dev,
            led_r: gpioa.pa7.into_push_pull_output(),
            led_g: gpioa.pa8.into_push_pull_output(),
            led_b: gpioa.pa9.into_push_pull_output(),
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
            Led_Counter: 0,
            Scale_modify: scale_modify,
            pmw3389,
            }      
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        loop {
            continue;
        }
    }

    //Increase or decrease the sensitivity of the mouse
    #[task(resources = [scl_minus, scl_plus, Scaler, Scale_modify], priority = 1, schedule = [toggle_speed])]
    fn toggle_speed(mut cx: toggle_speed::Context) {

            if cx.resources.scl_plus.is_high().unwrap() && !*cx.resources.Scale_modify {
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
            if cx.resources.scl_minus.is_high().unwrap() && !*cx.resources.Scale_modify {
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
    
    // Builds and sends the mouse report
    #[task(binds=OTG_FS, resources = [Led_Counter, led_r, led_g, led_b, r_click, l_click, w_click, M1_click, M2_click, scroll_up, scroll_down, Scaler, hid, pmw3389, usb_dev], priority = 2)]
    fn report(cx: report::Context) {
        static mut PREV_UP: bool = false;
        static mut PREV_DOWN: bool = false;

        //Setting up the resources
        let Led_Counter = cx.resources.Led_Counter;
        let myScaler = cx.resources.Scaler;
        let hid = cx.resources.hid;
        let led_r = cx.resources.led_r;
        let led_g = cx.resources.led_g;
        let led_b = cx.resources.led_b;
        let r_click = cx.resources.r_click;
        let l_click = cx.resources.l_click;
        let w_click = cx.resources.w_click;
        let M1_click = cx.resources.M1_click;
        let M2_click = cx.resources.M2_click;
        let usb_dev = cx.resources.usb_dev;
        let up = cx.resources.scroll_up.is_high().unwrap();
        let down = cx.resources.scroll_down.is_high().unwrap();
        let wheel_count = calculate_scroll(up, down, *PREV_UP, *PREV_DOWN);
        *PREV_UP = up;
        *PREV_DOWN = down;

        //LEDs
        let state: i8;
        if *Led_Counter == 1000{
            *Led_Counter = 0 as u16;
            if l_click.is_high().unwrap(){
                if led_r.is_high().unwrap(){
                    state = 1;
                }
                else{
                    state = 2;
                }
            }
            else if l_click.is_low().unwrap() && r_click.is_high().unwrap(){
                if led_b.is_high().unwrap(){
                    state = 3;
                }
                else{
                    state = 4;
                }
            }
            else{
                if led_g.is_high().unwrap(){
                    state = 5;
                }
                else{
                    state = 6;
                }
            }
            toggle_led(state, led_r, led_g, led_b);
            }
        else{
            *Led_Counter = *Led_Counter + 1;
        }
        
        // Fetch values from the sensor
        let (x, y) = cx.resources.pmw3389.read_status().unwrap();

        // Build and send the report
        let report = PMouseReport {
            buttons: ((M1_click.is_high().unwrap() as u8) << 4
                | (M2_click.is_high().unwrap() as u8) << 3
                | (w_click.is_high().unwrap() as u8) << 2
                | (r_click.is_high().unwrap() as u8) << 1
                | (l_click.is_high().unwrap() as u8)),
            x: ((-x as f32 * *myScaler) as i8)>>1,
            y: ((-y as f32 * *myScaler) as i8)>>1,
            wheel: wheel_count,
        };
        hid.push_input(&report).ok();
        
        if usb_dev.poll(&mut [hid]) {
            return;
        }
   
        
    }

    extern "C" {
        fn EXTI1();
    }
    
    extern "C" {
        fn EXTI2();
    }
};

// Toggles the LEDs according to the state
fn toggle_led<E>(state: i8, led_r: &mut dyn OutputPin<Error = E>, led_g: &mut dyn OutputPin<Error = E>, led_b: &mut dyn OutputPin<Error = E>) {
    if state == 1{
        led_r.set_low();
        led_g.set_high();
        led_b.set_high();
    }
    if state == 2{
        led_r.set_high();
        led_g.set_high();
        led_b.set_high();
    }
    if state == 3{
        led_r.set_high();
        led_g.set_high();
        led_b.set_low();
    }
    if state == 4{
        led_r.set_high();
        led_g.set_high();
        led_b.set_high();
    }
    if state == 5{
        led_r.set_high();
        led_g.set_low();
        led_b.set_high();
    }
    if state == 6{
        led_r.set_high();
        led_g.set_high();
        led_b.set_high();
    };
}

// Calculates the scroll direction depending on an incremental encoder
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
