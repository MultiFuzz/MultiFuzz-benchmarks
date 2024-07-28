# MultiFuzz crash analysis

This document contains analysis for all the bugs listed in the paper (Table 3). For each bug, there is an associated input found by the fuzzer in the [crashes](./crashes) directory that can be replayed by running the `./replay.sh` script to reproduce crash.

MultiFuzz also includes a GDB-stub to allow a debugger to connected for futher analysis:

```bash
GDB_BIND=127.0.0.1:9999 ./replay.sh crashes/riot-gnrc_networking/icmpv6_echo_send_overflow
```

```bash
arm-none-eabi-gdb -ex 'target remote :9999' ./benchmarks/MultiFuzz/riot-ccn-lite-relay/ccn-lite-relay.elf
```

MultiFuzz includes basic reverse-execution support in the form of the the `rsi` command (reverse-step-instruction), which we found very useful during bug triaging.

In some of the analysis below, we also report a '`icount`' value for moments during the firmware execution that are relavent to the underlying bug/crash. These can be navigated to in GDB by running the following monitor command:

```
monitor goto <icount>
```

Note: GDB does not automatically update after running a monitor command, so running at least one `si` or `rsi` command after a `monitor goto` is recommended.


## Overview

**MultiFuzz Binaries (new)**

| Target                       | Description
| -------------                | ------------------------------------------------------------------
| [RIOT GNRC Networking][gnrc] | [Integer underflow in `icmpv6_echo_send`][gnrc_bug_0]
| [RIOT CCN-Lite][ccn_lite]    | [Stdio initialization race][ccn_bug_0]
| [RIOT CCN-Lite][ccn_lite]    | [Issue with `%` encoded characters in `ccnl_cs`][ccn_bug_1]
| [RIOT CCN-Lite][ccn_lite]    | [Reinitialization of shared global timer][ccn_bug_2]
| [RIOT CCN-Lite][ccn_lite]    | [Missing removal from `evtimer` struct][ccn_bug_3]
| [RIOT CCN-Lite][ccn_lite]    | [(FP) Uninitialized RTC Overflow Callback][ccn_bug_4]

[gnrc]: #riot-grnc-networking
[gnrc_bug_0]: #integer-underflow-in-icmpv6_echo_send


[ccn_lite]: #riot-ccn-lite
[ccn_bug_0]: #stdio-initialization-race
[ccn_bug_1]: #issue-with--encoded-characters-in-ccnl_cs
[ccn_bug_2]: #reinitialization-of-shared-global-timer
[ccn_bug_3]: #missing-removal-from-evtimer-struct
[ccn_bug_4]: #uninitialized-rtc-overflow-callback-false-positive


**HALucinator/P2IM/uEmu Binaries**

| Target                | Description
| -------------         | ------------------------------------------------------------------
| [Gateway][gateway]    | [Incorrect handling of zero length sysex messages][gateway_bug_5]
| [6LoWPAN_Sender][6rxtx]   | [Fragment offset is not bounds-checked in `sicslowpan::input`][6rxtx_bug_2]
| [6LoWPAN_Sender][6rxtx]   | [(FP) SERCOM0 initialization race][6rxtx_bug_1]
| [6LoWPAN_Sender][6rxtx]   | [(FP) Unbounded recursion when obtaining clock rate][6rxtx_bug_3]
| [uEmu.GPSTracker][gps]    | [`strtok` not checked for NULL in `gsm_get_imei`][gps_bug_2]
| [uEmu.GPSTracker][gps]    | [`strstr` not checked for NULL in `sms_check`][gps_bug_3]
| [uEmu.GPSTracker][gps]    | [`strstr` not checked for NULL in `gsm_get_time`][gps_bug_4]
| [uEmu.GPSTracker][gps]    | [`strtok` not checked for NULL in `gsm_get_time`][gps_bug_5]
| [utasker_MODBUS][modbus]  | [Direct manipulation of memory using I/O menu][modbus_bug_1]
| [utasker_USB][usb]        | [(FP) Out-of-bounds access in `fnUSB_handle_frame` from `GRXSTSPR.CHNUM` value][usb_bug_1]
| [utasker_USB][usb]        | [Out-of-bounds access from interface index in `control_callback`][usb_bug_2]
| [utasker_USB][usb]        | [(FP) Uninitialized usage of `SerialHandle`][usb_bug_3]
| [utasker_USB][usb]        | [Direct manipulation of memory using I/O menu][usb_bug_4]
| [Zephyr_SocketCan][zepyhr]| [`canbus` subcommands fail to validate argument count][zepyhr_bug_2]
| [Zephyr_SocketCan][zepyhr]| [(FP) Out-of-bounds write in `can_stm32_attach`][zepyhr_bug_3]
| [Zephyr_SocketCan][zepyhr]| [Incorrect comparison used for bounds check in `execute`][zepyhr_bug_4]
| [Zephyr_SocketCan][zepyhr]| [`net pkt` command dereferences a user provided pointer][zepyhr_bug_5]
| [Zephyr_SocketCan][zepyhr]| [`canbus` subcommands fail to verify device type][zepyhr_bug_6]
| [Zephyr_SocketCan][zepyhr]| [`pwm` subcommand fail to verify device type][zepyhr_bug_7]


<!-- False positives: FW 21, FW 22, FW 25, FW 26, FW 40, FW 41, FW 42, FW 27, FW 45 -->

<!-- Binaries with (known) exploitable bugs that allow additional code to be hit (be careful when calculating coverage): PLC, Heat_Press, Thermostat, RF_Door_Lock, utasker_USB -->

[gateway]: #Gateway
[gateway_bug_5]: #incorrect-handling-of-zero-length-sysex-messages

[6rxtx]: #6lowpan-senderreceiver
[6rxtx_bug_1]: #sercom0-initialization-race-false-positive
[6rxtx_bug_2]: #fragment-offset-is-not-bounds-checked-in-sicslowpaninput
[6rxtx_bug_3]: #unbounded-recursion-when-obtaining-clock-rate-false-positive

[gps]: #GPSTracker-OpenTracker-320
[gps_bug_0]: #stack-overflow-in-USB_SendStringDescriptor
[gps_bug_1]: #return-value-of-strstr-not-checked-for-null-in-gsm_get_imei
[gps_bug_2]: #return-value-of-strtok-not-checked-for-null-in-gsm_get_imei
[gps_bug_3]: #return-value-of-strstr-not-checked-for-null-in-sms_check
[gps_bug_4]: #return-value-of-strstr-not-checked-for-null-in-gsm_get_time
[gps_bug_5]: #return-value-of-strtok-not-checked-for-null-in-gsm_get_time


[modbus]: #utasker-MODBUS
[modbus_bug_0]: #uninitialized-usage-of-serialhandle-false-positive
[modbus_bug_1]: #direct-manipulation-of-memory-using-io-menu
[modbus_bug_2]: #fnSetFlashOption-attempts-to-execute-a-function-copied-to-the-stack-false-positive


[usb]: #utasker-USB
[usb_bug_0]: #buffer-overflow-in-fnextractfifo-false-positive
[fw_bug_27]: https://github.com/fuzzware-fuzzer/fuzzware-experiments/tree/main/04-crash-analysis/27
[usb_bug_1]: #out-of-bounds-access-in-fnusb_handle_frame-from-grxstsprchnum-value-false-positive
[fw_bug_45]: https://github.com/fuzzware-fuzzer/fuzzware-experiments/tree/main/04-crash-analysis/45
[usb_bug_2]: #out-of-bounds-access-from-interface-index-in-control_callback
[usb_bug_3]: #uninitialized-usage-of-serialhandle-false-positive-1
[usb_bug_4]: #direct-manipulation-of-memory-using-io-menu-1

[zepyhr]: #Zepyhr_SocketCan
[zepyhr_bug_0]: #unchecked-error-handler-in-z_impl_can_attach_msgq
[zepyhr_bug_1]: #initialization-race-in-log_backend_enable-false-positive
[zepyhr_bug_2]: #canbus-subcommands-fail-to-validate-argument-count-correctly
[zepyhr_bug_3]: #out-of-bounds-write-in-can_stm32_attach-false-positive
[zepyhr_bug_4]: #incorrect-comparison-operator-used-for-bounds-check-in-execute
[zepyhr_bug_5]: #net-pkt-command-dereferences-a-user-provided-pointer
[zepyhr_bug_6]: #canbus-subcommands-fail-to-verify-device-type
[zepyhr_bug_7]: #pwm-subcommands-fail-to-verify-device-type


## RIOT GNRC Networking

### Integer underflow in `icmpv6_echo_send`

It is possible for an integer overflow of `len`/`data_len` to occur as part of `gnrc_icmpv6_echo_send`/`gnrc_icmpv6_echo_build` when computing the total length of the packet including the header i.e. `data_len + sizeof(imcpv6_echo_t)` at [gnrc_icmpv6_echo.c:34](https://github.com/RIOT-OS/RIOT/blob/d7dba6206b783d09e3fef392caa6e3b65c15f00e/sys/net/gnrc/network_layer/icmpv6/echo/gnrc_icmpv6_echo.c#L34)

Depending on the value used this results in either either a null pointer dereference at [gnrc_icmpv6.c:136](https://github.com/RIOT-OS/RIOT/blob/d7dba6206b783d09e3fef392caa6e3b65c15f00e/sys/net/gnrc/network_layer/icmpv6/gnrc_icmpv6.c#L136) (if `len == (SIZE_MAX - 8)`) or a buffer overflow at [gnrc_icmpv6_echo.c:181](https://github.com/RIOT-OS/RIOT/blob/d7dba6206b783d09e3fef392caa6e3b65c15f00e/sys/net/gnrc/network_layer/icmpv6/echo/gnrc_icmpv6_echo.c#L181) (if `(SIZE_MAX - 8) < len <= SIZE_MAX`).

Replay: `./replay.sh crashes/riot-gnrc_networking/icmpv6_echo_send_overflow`

In the provided replay file, the fuzzer finds an input that runs a `ping -s -7 ::1` via the CLI interface, the `-s` flag is used to set the size of the ping packet based on the the subsequent argument which is parsed using `atoi` at [gnrc_icmpv6_echo.c:210](https://github.com/RIOT-OS/RIOT/blob/d7dba6206b783d09e3fef392caa6e3b65c15f00e/sys/shell/cmds/gnrc_icmpv6_echo.c#L210) (which allows negative numbers) resulting in a size of `0xfffffffc`.

This results in the `_fill_payload` overflowing the allocated buffer, and eventually corrupting the `isr_ctx@0x20003848` global variable at (icount=247318):

```
0x08001636: _fill_payload at /RIOT/sys/net/gnrc/network_layer/icmpv6/echo/gnrc_icmpv6_echo.c:121
0x0800163a: gnrc_icmpv6_echo_send at /RIOT/sys/net/gnrc/network_layer/icmpv6/echo/gnrc_icmpv6_echo.c:181
0x08009136: _pinger at /RIOT/sys/shell/cmds/gnrc_icmpv6_echo.c:274
0x0800921c: _gnrc_icmpv6_ping at /RIOT/sys/shell/cmds/gnrc_icmpv6_echo.c:106
0x08008ec6: handle_input_line at /RIOT/sys/shell/shell.c:325
0x080001aa: shell_run_forever at /RIOT/sys/include/shell.h:179
```

Causing an the interrupt handler to an jump to an invalid address at (icount=299295):

```
UnhandledException(code=InvalidInstruction)
0x0800883e: irq_handler at /RIOT/cpu/stm32/periph/uart.c:514
```

## RIOT CCN-Lite

### Stdio initialization race

The reentrant stdio functions in `newlib` initialize shared data in the `reent` data structure via the [`__sinit`](https://github.com/bminor/newlib/blob/5da71b6059956a8f20a6be02e82867aa28aa3880/newlib/libc/stdio/findfp.c#L245) function, the first time an IO function (e.g., `puts`) is used. Normally, before `main`, RIOT prints a message using `puts` (i.e., "This is RIOT" + version), which ends up calling `__sinit` before user code is executed, avoiding most of the issues with initialization.

However, if a interrupt occurs before `main` or the `CONFIG_SKIP_BOOT_MSG` compilation flag is set, then initialization races are possible. If a two regular tasks, or a task and an interrupt handler attempt to print at a roughly similar time, then it is possible<sup>1</sup> for an IO function to be executed with a partially initialized `reent` object. This causes various crashes depending on how much of the structure has been initialized.

For the crash to occur, an interrupt or task preemption needs to within the region of code: [findfp.c:60-67](https://github.com/32bitmicro/newlib-nano-2/blob/0c5e24765fb745dc7c59f00248680c22357ffd55/newlib/libc/stdio/findfp.c#L60-L67), because of checks on the `stdout` FILE structure that occur as part of `puts` (e.g., `SWR` must be set in `flags`),

<sub>1. There is a lock (using `__sinit_lock_acquire`) that guards `sinit`, however this calls `__retarget_lock_acquire_recursive` which does nothing when targeting a bare-metal ARM embedded system.</sub>

Replay: `./replay.sh crashes/riot-ccn-lite-relay/init_race`

In the provided replay file, a BLE IRQ is triggered while the _main thread_ is part way through stdio initialization (icount = 415277):

```
<signal handler called>
0x0001989e: memset
0x000195c0: __sfmoreglue
0x000196d4: __sfp
0x00019624: __sinit
0x00019ac4: _puts_r
0x00000c78: main_trampoline at /RIOT/core/lib/init.c:60
0x000009cc: sched_switch at /RIOT/core/sched.c:303
```

The BLE ISR then triggers an assert, which attempts to print to the serial port, causing a crash at (icount=415379):

```
UnhandledException(code=WritePerm, value=0x1ec48)
0x0001a0cc: __swsetup_r
0x0001a6e0: _vfprintf_r
0x00019a7e: printf
0x00000c5a: _assert_panic at /RIOT/cpu/cortexm_common/include/cpu.h:138
0x00008a28: ble_phy_isr at /RIOT/build/pkg/nimble/nimble/drivers/nrf52/src/ble_phy.c:1389
```

### Issue with `%` encoded characters in `ccnl_cs`

As part of processing URIs, escape sequences using `%` are handled as part of [ccnl_URItoComponents](https://github.com/cn-uofbasel/ccn-lite/blob/68c9a39d29b2f4ed717b311525709f29f602afd3/src/ccnl-core/src/ccnl-prefix.c#L294). However, the code does not restrict allowable escape characters, which enables a NULL byte to be encoded with `%00` (invalid escape codes, such as `%gg`, are treated as NULL as well).

This causes encoding issues in [ccnl_ndntlv_prependName](https://github.com/cn-uofbasel/ccn-lite/blob/68c9a39d29b2f4ed717b311525709f29f602afd3/src/ccnl-pkt/src/ccnl-pkt-ndntlv.c#L516). A component starting with a null character `\0`  is treated as a control character `NDT_Marker_SegmentNumber` as part of [ccnl_ndntlv_bytes2pkt](https://github.com/cn-uofbasel/ccn-lite/blob/68c9a39d29b2f4ed717b311525709f29f602afd3/src/ccnl-pkt/src/ccnl-pkt-ndntlv.c#L182-L192C22).

The rest of the component is then interpreted as a chunk number (which is likely to be invalid). If the chunk number is large enough, then `chunknum > UINT32_MAX` will fail,  causing a NULL pointer to be returned from `ccnl_ndntlv_bytes2pkt`. Finally, when handling the `ccnl_cs` command [_ccnl_content](https://github.com/RIOT-OS/RIOT/blob/e690ef4c1298d90bc323b765c000bb090544fb9f/sys/shell/cmds/ccn-lite-utils.c#L136) fails to check for a NULL return value, and stores a NULL packet in the content cache.

If a command is then run to display the values stored in the content cache. Then the code will attempt to dereference the NULL packet, resulting in a crash. (Note: the address 0x0 is mapped, but it eventually the firmware crashes after dereferencing additional pointers).

Replay: `./replay.sh crashes/riot-ccn-lite-relay/bad_ccnl_cs`

In the replay file the fuzzer finds an input that runs the following commands (the exact string has been changed to avoid non-ascii characters):

```
> ccnl_cs %00uristring content
> ccnl_cs
```

The null packet is added to the content cache at:

```
0x00015f08: ccnl_content_new at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-content.c:60
0x00013732: _ccnl_content at /RIOT/sys/shell/cmds/ccn-lite-utils.c:137
0x000134d6: handle_input_line at /RIOT/sys/shell/shell.c:325
0x00000130: shell_run_forever at /RIOT/sys/include/shell.h:179
```

Causing a crash at:

```
UnhandledException(code=ReadUnmapped, value=0x1a466aff)
0x000164f2: ccnl_prefix_to_str_detailed at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-prefix.c:558
0x00016c38: ccnl_cs_dump at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-relay.c:967
0x00013658: _ccnl_content at /RIOT/sys/shell/cmds/ccn-lite-utils.c:90
0x000134d6: handle_input_line at /RIOT/sys/shell/shell.c:325
0x00000130: shell_run_forever at /RIOT/sys/include/shell.h:179
```


### Reinitialization of shared global timer

Global state, including a timer (`evtimer`), is initialized as part of `ccnl_start` ([ccn-lite-riot.c:452](https://github.com/cn-uofbasel/ccn-lite/blob/da0d9de8d82349dff845acc62d37242dd09b3d3d/src/ccnl-riot/src/ccn-lite-riot.c#L452)). After initializing the timer, the firmware also creates a loopback interface, and registers a timeout event with the event loop. However, running the `ccnl_open` command calls `ccnl_start` again, which reinitialize the global state at [ccn-lite-utils.c:65](https://github.com/RIOT-OS/RIOT/blob/e690ef4c1298d90bc323b765c000bb090544fb9f/sys/shell/cmds/ccn-lite-utils.c#L65). This causes `evtimer.events` to be set to NULL, however pending timeouts may attempt to read `evtimer.events` causing a NULL pointer dereference resulting in a crash.

Replay: `./replay.sh crashes/riot-ccn-lite-relay/ccnl_open`

In the provided replay file, `evtimer.events@0x20008800` is set to null at icount=296378:

```
0x00001692 in evtimer_init at /RIOT/sys/evtimer/evtimer.c:221
0x00015d38 in evtimer_init_msg at /RIOT/sys/include/evtimer_msg.h:80
0x00015d38 in ccnl_start at /RIOT/build/pkg/ccn-lite/src/ccnl-riot/src/ccn-lite-riot.c:451
0x00013570 in _ccnl_open at /RIOT/sys/shell/cmds/ccn-lite-utils.c:65
0x000134d6 in handle_input_line at /RIOT/sys/shell/shell.c:325
0x00000130 in shell_run_forever at /RIOT/sys/include/shell.h:179

   0x00001690 <+0>:     ldr     r3, [pc, #12]   ; (0x16a0 <evtimer_init+16>)
=> 0x00001692 <+2>:     str     r1, [r0, #20]
   0x00001694 <+4>:     strd    r3, r0, [r0, #8]
   0x00001698 <+8>:     movs    r3, #0
   0x0000169a <+10>:    str     r3, [r0, #24]
   0x0000169c <+12>:    bx      lr
```

Causing a crash at:

```
UnhandledException(code=WritePerm, value=0x4) (icount = 302126), active_irq = 33
0x000015c0 in _evtimer_handler at /RIOT/sys/evtimer/evtimer.c:209
0x000154f6 in ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x000154f6 in ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x00012a90 in isr_rtc1 at /RIOT/cpu/nrf5x_common/periph/rtt.c:121
<signal handler>
```

### Missing removal from `evtimer` struct

This issue is related to the way the `ccn_lite` library is integrated into RIOT using the `evtimer` interface. As part of creating a ccnl interface, the code registers a timeout event to reset the interface (when configured for RIOT) via `ccnl_evtimer_reset_face_timeout` ([ccnl-relay.c:71](https://github.com/cn-uofbasel/ccn-lite/blob/da0d9de8d82349dff845acc62d37242dd09b3d3d/src/ccnl-core/src/ccnl-relay.c#L71-L73) > [ccn-lite-riot.h:299](https://github.com/cn-uofbasel/ccn-lite/blob/68c9a39d29b2f4ed717b311525709f29f602afd3/src/ccnl-riot/include/ccn-lite-riot.h#L299)).

The interface is removed using [`ccnl_face_remove`](https://github.com/cn-uofbasel/ccn-lite/blob/da0d9de8d82349dff845acc62d37242dd09b3d3d/src/ccnl-core/src/ccnl-relay.c#L132), however it is missing code for removing the event from the RIOT evtimer.

Other similar functions that register callbacks with the RIOT event handler, call RIOT-specific cleanup functions, that remove the event from the `evtimer` struct. e.g., `ccnl_interest_remove` (e.g.: [ccnl-relay.c:360](https://github.com/cn-uofbasel/ccn-lite/blob/da0d9de8d82349dff845acc62d37242dd09b3d3d/src/ccnl-core/src/ccnl-relay.c#L360-L362)). This issue also seems similar to an existing patch: [0004-ccnl_content_remove-Fix-use-after-free.patch](https://github.com/RIOT-OS/RIOT/blob/master/pkg/ccn-lite/patches/0004-ccnl_content_remove-Fix-use-after-free.patch)

Without this cleanup, after the call to `free` at [ccnl-relay.c:191](https://github.com/cn-uofbasel/ccn-lite/blob/da0d9de8d82349dff845acc62d37242dd09b3d3d/src/ccnl-core/src/ccnl-relay.c#L191), the reference created at [ccn-lite-riot.h:299](https://github.com/cn-uofbasel/ccn-lite/blob/68c9a39d29b2f4ed717b311525709f29f602afd3/src/ccnl-riot/include/ccn-lite-riot.h#L299) is dangling.

Replay: `./replay.sh crashes/riot-ccn-lite-relay/dangling_evtimer`

In the provided replay file, the free occurs at (icount=333586):

```
0x0000259c: ccnl_face_remove at __wrap_free ; (ptr = 0x200097a0)
0x000168f6: ccnl_face_remove at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-relay.c:191
0x00015cf4: _ccnl_event_loop at /RIOT/build/pkg/ccn-lite/src/ccnl-riot/src/ccn-lite-riot.c:435
0x000009cc: sched_switch at /RIOT/core/sched.c:303
```

The dangling reference overlaps with a later allocation of a `struct ccnl_face_s` object at (icount=345109):

```
__wrap_calloc returned 0x200097a8
0x00016656: ccnl_get_face_or_create at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-relay.c:93
0x00016edc: ccnl_interest_broadcast at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-relay.c:486
0x00016f4a: ccnl_interest_propagate at /RIOT/build/pkg/ccn-lite/src/ccnl-core/src/ccnl-relay.c:458
0x00017916: ccnl_fwd_handleInterest at /RIOT/build/pkg/ccn-lite/src/ccnl-fwd/src/ccnl-fwd.c:294
0x00015c10: _ccnl_event_loop at /RIOT/build/pkg/ccn-lite/src/ccnl-riot/src/ccn-lite-riot.c:379
0x000009cc: sched_switch at /RIOT/core/sched.c:303
```

The `event.msg` field of the dangling pointer overlaps with the `event.next` field of the newly allocated object:

```
&((struct ccnl_face_s*)(0x200097a0))->evtmsg_timeout.msg        = 0x200097e4
&((struct ccnl_face_s*)(0x200097a8))->evtmsg_timeout.event.next = 0x200097e4
```

When a callback function for the dangling event is later called (from a timeout event) it corrupts the overlapping allocation (icount=350093):

```
msg.sender_pid@0x200097ec = 0x21 (icount=350093)
0x000005a2: msg_send_int at /RIOT/core/msg.c:235
0x000015d2: _evtimer_handler at /RIOT/sys/evtimer/evtimer.c:213
0x000154f6: ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x000154f6: ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x00012a90: isr_rtc1 at /RIOT/cpu/nrf5x_common/periph/rtt.c:121
```

This eventually results in a crash when the overlapping allocation is later used:

```
UnhandledException(code=WritePerm, value=0x29) (icount = 350576), active_irq = 33
0x000005a2: msg_send_int (m=0x29, target_pid=0) at /RIOT/core/msg.c:235
0x000015d2: _evtimer_handler at /RIOT/sys/evtimer/evtimer.c:213
0x000154f6: ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x000154f6: ztimer_handler at /RIOT/sys/ztimer/core.c:487
0x00012a90: isr_rtc1 at /RIOT/cpu/nrf5x_common/periph/rtt.c:121
```

### Uninitialized RTC Overflow Callback (false positive)

Replay: `./replay.sh crashes/riot-ccn-lite-relay/rtc_overflow`

The `RTC ISR OVRFLW` event can be trigged calling null overflow handler, however this is a false positive since the `OVRFLW` event for the `RTC` peripheral is disabled by default on the target MCU, and never enabled by the firmware.

This results in a crash at: `0x12a98: isr_rtc1 at /RIOT/cpu/nrf5x_common/periph/rtt.c:125:28 (active_irq = 33)`


## Gateway

#### Incorrect handling of zero length sysex messages

The firmware fails check for zero length sysex messages in `processSysexMessage`. Normally this doesn't cause an issue since the command is read from the first byte in [`parserBuffer`](https://github.com/firmata/arduino/blob/c285135275c4dd4f0d0bbf82da3c5844a29eaf07/FirmataParser.cpp#L429) which is zero on initialization (not maching any command handler). However, since the buffer is not zeroed _between_ commands, zero length sysex messages will cause the previous handler to be executed. If the previous sysex command was a `'q'` command (e.g., given the sequence `[f0 71 f7] [f0 f7]`) this causes an integer underflow when the size of the command is computed and passed to `decodeByteStream`.

Replay: `./replay.sh crashes/Gateway/zero_length_sysex`

In the replay, the underflow occurs at (icount=189567):

```
0x000800341e: firmata::FirmataParser::processSysexMessage() at /Firmata/FirmataParser.cpp:448
0x000800349b: firmata::FirmataParser::parse(unsigned char) at /Firmata/FirmataParser.cpp:90
0x000800349a: firmata::FirmataParser::parse(unsigned char) at /Firmata/FirmataParser.cpp:90
0x0008002ef4: firmata::FirmataClass::processInput() at /Firmata/Firmata.cpp:256
0x0008002310: loop at Gateway/StandardFirmata.ino:796
0x0008008f6e: main at STM32/hardware/stm32/1.3.0/cores/arduino/main.cpp:68
```

This causes `firmata::FirmataParser::decodeByteStream` to be called with a byte count of `0xffffffff` (icount=189571) overflowing a buffer and corrupting global memory, including the `uart_handlers` array, which causes a crash when an interrupt is triggered (icount = 201991):

```
UnhandledException(code=ReadUnmapped, value=0x800080), active_irq = 54
0x00080069f4: HAL_UART_IRQHandler at STM32/hardware/stm32/1.3.0/system/Drivers/STM32F1xx_HAL_Driver/Src/stm32f1xx_hal_uart.c:1557
0x0008008828: USART2_IRQHandler at STM32/hardware/stm32/1.3.0/cores/arduino/stm32/uart.c:835
<signal handler called>
0x0008003428: firmata::FirmataParser::processSysexMessage() at Firmata/FirmataParser.cpp:448
0x000800349a: firmata::FirmataParser::parse(unsigned char) at Firmata/FirmataParser.cpp:90
0x0008002ef4: firmata::FirmataClass::processInput() at Firmata/Firmata.cpp:256
0x0008002310: loop at Gateway/StandardFirmata.ino:796
0x0008008f6e: main at STM32/hardware/stm32/1.3.0/cores/arduino/main.cpp:68
0x000800368e: Reset_Handler+0x30
```

## 6LoWPAN Sender/Receiver


### Fragment offset is not bounds-checked in `sicslowpan::input`

The firmware receives 6lowpan packets from the SPI data register storing the packet in a buffer (the address of the buffer `0x2000120b` is stored in `packetbuf_ptr`). This 6lowpan packet is then decoded as part of the `0x4690: input` function. Packets can be split into multiple fragments, so to support packet reassembly, the packet header defines 3 fields:

* `frag_size = ((packetbuf_ptr[0] as u16 << 8) | (packetbuf_ptr[1] as u16)) & 0x7ff`
* `frag_tag = (packetbuf_ptr[2] as u16 << 8) | (packetbuf_ptr[3] as u16)`
* `frag_offset = packetbuf_ptr[4]`

The `frag_offset` field is used to copy the packet payload to the correct position in the `sicslowpanbuf` buffer. However, the firmware fails to check that the offset is in bounds, causing corruption during a `memcpy` operation.

Replay: `./replay crashes/6LoWPAN_Receiver/fragment_offset_oob`

In the replay file at (icount=1722602), we end up with a packet with `frag_offset = 0xc1` causing the destination of the `memcpy` function to be `0x200029d0` which is an out-of-bounds address overlapping with the `uip_ds6_timer_periodic` global variable.

Consequently, `0x200029d8: uip_ds6_timer_periodic.next` is corrupted as part of the memcpy operation, setting it to `0x7a7a8383`. This object is part of link-list of timers starting with `timerlist = (etimer *)0x20002ffc`. `timerlist` is iterated as part of `process_thread_etimer_process` causing a crash when `uip_ds6_timer_periodic.next` is traversed during the call to `timer_expired` (icount=1767287):

```
UnhandledException(code=ReadUnmapped, value=0x7a7a8383)
0x000000c7c4: timer_expired at ./src/ASF/thirdparty/wireless/SmartConnect_6LoWPAN/core/sys\timer.c:125
0x000000c2d6: process_thread_etimer_process at ./src/ASF/thirdparty/wireless/SmartConnect_6LoWPAN/core/sys\etimer.c:117
0x000000c4d8: call_process at ./src/ASF/thirdparty/wireless/SmartConnect_6LoWPAN/core/sys\process.c:190
0x000000c52c: do_poll at ./src/ASF/thirdparty/wireless/SmartConnect_6LoWPAN/core/sys\process.c:235
0x000000c5d8: process_run at ./src/ASF/thirdparty/wireless/SmartConnect_6LoWPAN/core/sys\process.c:306
0x000000d758: main at ./src\udp-unicast-receiver-main.c:268
0x00000028e6: Reset_Handler at ./src/ASF/sam0/utils/cmsis/samr21/source/gcc\startup_samr21.c:255
```

### SERCOM0 initialization race (false positive)

This initialization race false-positive is similar to the following [Fuzzware](https://github.com/fuzzware-fuzzer/fuzzware-experiments/tree/bee7f80b104b42d5181c3679fce93b14e4e7b187/04-crash-analysis) crashes: 21, 22, 25, 26, 40, 41, 42. The underlying issue is that interrupts must be enabled in _both_ the NVIC and the peripheral itself, however Fuzzware's interrupt controller (which we re-use) only checks that an interrupt is enable in the NVIC.

The `sio2host_init` function enables the `SERCOM0` interrupt vector in the NVIC (at `0x2b3a`), then later sets the interrupt handler using the `_sercom_set_handler` function (at `0x2b8e`). If a `SERCOM0` interrupt is trigged before the hander is set it causes a null pointer to be dereferenced crashing the firmware.

However, this is a false positive, since interrupt generation from the peripheral itself (`INTENSET` is not set until `0x2b94`).

Replay: `./replay.sh crashes/6LoWPAN_Receiver/sercom0_init`

```
UnhandledException(code=InvalidInstruction, value=0x0) (icount = 21192), active_irq = 25
0x0000000000: <unknown>
0x00000011eb: SERCOM0_Handler at src/ASF/sam0/drivers/sercom\sercom_interrupt.c:141
0x0000002b90: sio2host_init at src/ASF/thirdparty/wireless/addons/sio2host/uart\sio2host.c:110
0x000000d530: main at src\udp-unicast-receiver-main.c:139
```

## Unbounded recursion when obtaining clock rate (false positive)

The call chain `system_gclk_chan_get_hz->system_gclk_gen_get_hz->system_clock_source_get_hz` contains a cycle enabling unbounded stack growth. Given enough data this overflows the stack and corrupts global variables including the callback table for external interrupts, causing a crash when an interrupt is triggered.

However, this is a false-positive since this requires that clock source always returns the same value (`SYSTEM_CLOCK_SOURCE_DFLL`). On real hardware this depends on the value of certain control registers which a set to specific values.

Replay: `./replay.sh crashes/6LoWPAN_Receiver/clock_rate_recursion`

In the replay file, the stack frame looks like:

```
... stack frame repeats ...
0x0000001c56: system_clock_source_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\clock.c:207
0x00000021b0: system_gclk_gen_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\gclk.c:308
0x00000022d6: system_gclk_chan_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\gclk.c:521
0x0000001c56: system_clock_source_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\clock.c:207
0x00000021b0: system_gclk_gen_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\gclk.c:308
0x00000022d6: system_gclk_chan_get_hz at src/ASF/sam0/drivers/system/clock/clock_samd21_r21_da_ha1\gclk.c:521
0x0000001868: _usart_set_config at src/ASF/sam0/drivers/sercom/usart\usart.c:151
0x0000002b16: usart_serial_init at src/ASF/common/services/serial/sam0_usart\usart_serial.h:77
0x000000d530: main at src/udp-unicast-receiver-main.c:139
0x00000028e6: Reset_Handler at src/ASF/sam0/utils/cmsis/samr21/source/gcc\startup_samr21.c:255
```

Resulting in a crash at:

```
UnhandledException(code=InvalidInstruction, value=0x68) (icount = 81139), active_irq = 20
0x000000039e: EIC_Handler at src/ASF/sam0/drivers/extint\extint_callback.c:228
<signal handler called>
```

## GPS Tracker

### Return value of `strtok` not checked for NULL in `gsm_get_imei`

After searching for `"AT+GSN\r\r\n"` in the `gsm_get_imei` function using `strstr`, the firmware then attempts split the remainder of the response up to the next `\r` character using `strtok` at `0x80ae4`. If there is no additional `\r` characters in the response, then `strtok` will return NULL, however the firmware fails to check for this case, causing a NULL pointer dereference when the return value is used as a parameter for `strlcpy`.

Replay: `./replay crashes/uEmu/GPSTracker/gsm_get_imei_strtok`

```
UnhandledException(code=ReadUnmapped, value=0x0)
0x00000859ec: strlcpy+0x6 ; tail call in gsm_get_imei
0x0000081142: gsm_config() at OpenTracker\gsm.ino:195
0x0000082452: setup at OpenTracker\OpenTracker.ino:118
0x000008480a: main at ArduinoData\packages\opentracker\hardware\sam\1.0.5\cores\arduino\main.cpp:57
0x00000837f6: Reset_Handler+0x54
```


#### Return value of `strstr` not checked for NULL in `sms_check`

In the `sms_check` function, `strstr` is called to find the location of the first occurence of: `"+CMGR:"` in the response. However, the firmware never handles the case where the response does not contain `"+CMGR:"` which causes a crash when the return value for `strstr` is used as the argument of a later `strstr` call.

Replay: `./replay crashes/uEmu/GPSTracker/sms_check_strstr`

```
UnhandledException(code=ReadUnmapped, value=0x0)
0x0000085de0: strstr+0x3
0x0000081cf8: sms_check() at OpenTracker\sms.ino:59
0x000008246e: setup at OpenTracker\OpenTracker.ino:151
0x000008480a: main at ArduinoData\packages\opentracker\hardware\sam\1.0.5\cores\arduino\main.cpp:57
0x00000837f6: Reset_Handler+0x54
```

Note `sms_check` is also called from the main loop which can cause a crash with slightly call stack.

Replay: `./replay crashes/uEmu/GPSTracker/sms_check_main_strstr`

```
0x0000085de0: strstr+0x3
0x0000081cf8: sms_check() at OpenTracker\sms.ino:59
0x0000082342: loop at OpenTracker\OpenTracker.ino:280
0x000008480e: main at ArduinoData\packages\opentracker\hardware\sam\1.0.5\cores\arduino\main.cpp:61
0x00000837f6: Reset_Handler+0x54
```

#### Return value of `strstr` not checked for NULL in `gsm_get_time`

In the `gsm_get_time` function, the firmware fails to handle the case where a call to `strstr` to find: `"+CCLK: \""` returns NULL.

Replay: `./replay crashes/uEmu/GPSTracker/gsm_get_time_strstr`

```
UnhandledException(code=ReadUnmapped, value=0x8)
0x0000085ff2: __strtok_r+0x5
0x0000080ebc: gsm_get_time() at OpenTracker\gsm.ino:297
0x000008211e: collect_all_data(int) at OpenTracker\data.ino:99
0x0000082306: loop at OpenTracker\OpenTracker.ino:251
0x000008480e: main at ArduinoData\packages\opentracker\hardware\sam\1.0.5\cores\arduino\main.cpp:61
0x00000837f6: Reset_Handler+0x54
```

#### Return value of `strtok` not checked for NULL in `gsm_get_time`

After searching for ``"+CCLK: \""`` in the `gms_get_time` the firmware searches for a closing quote character using `strtok`. However, if there is no closing quote character then a later call to `strlcpy` will dereference a NULL pointer.

Replay: `./replay crashes/uEmu/GPSTracker/gsm_get_time_strtok`

```
UnhandledException(code=ReadUnmapped, value=0x0)
0x00000859ec: strlcpy+0x7 ; tail call in gsm_get_time
0x000008211e: collect_all_data(int) at OpenTracker\data.ino:99
0x0000082306: loop at OpenTracker\OpenTracker.ino:251
0x000008480e: main at ArduinoData\packages\opentracker\hardware\sam\1.0.5\cores\arduino\main.cpp:61
0x00000837f6: Reset_Handler+0x54
```

<!-- USB False positive: 27, 45 -->

## uTasker MODBUS

## Direct manipulation of memory using I/O menu

The firmware the Input/Output menu supports direct manipulation of memory through several commands including:

* "md": Memory display
* "mm": Memory modify
* "mf": Memory fill
* "sf": Storage fill
* "sd": Storage display

Since these commands parse a hex encoded address from the command interface they enable reading/writing to arbitrary memory. Entering one of these commands with an invalid address causes the program to crash.

For example, the fuzzer finds a crash with the following input `"3\rmm musm l0\r"`

On startup the program starts at the main menu:

```
     Main menu
===================
1              Configure LAN interface
2              Configure serial interface
3              Go to I/O menu
4              Go to administration menu
5              Go to overview/statistics menu
```

By entering `3` the I/O menu is selected. Then `mm` selects the "Memory modify" subcommand. `musm` is parsed as an address using the `fnHexStrHex` function for an address of `0x6ec6` (note the `fnHexStrHex` does not validate that the digits are valid).

The firmware then attempts to write to `0x6ec6` resulting in the following crash:

```
UnhandledException(code=WriteUnmapped, value=0x6ec6)
0x000800fe60: fnDoCommand at uTasker-GIT-Kinetis\Applications\uTaskerV1.4\debug.c:7807.9
0x0008010be0: fnDoDebug at uTasker-GIT-Kinetis\Applications\uTaskerV1.4\debug.c:7914.17
0x0008012ad0: fnApplication at uTasker-GIT-Kinetis\Applications\uTaskerV1.4\application.c:1223.17
0x000800e6ee: uTaskerSchedule at uTasker-GIT-Kinetis\uTasker\uTasker.c:401.13
0x000800c214: main at uTasker-GIT-Kinetis\Hardware\STM32\STM32.c:410.9
0x000801581e: _call_main
0x0008015ed8: __iar_program_start
```

The other I/O commands can also causes crashes at slightly different locations, e.g.,

* `md`: ReadUnmapped crash at `0x000800ffbc`.
    - Replay: `./replay.sh crashes/utasker_MODBUS/memory_display`
* `mm`: WriteUnmapped crash at `0x000800fe66`.
    - Replay: `./replay.sh crashes/utasker_MODBUS/memory_modify`
* `mf`: WriteUmnapped crash at `0x000800fe94`
    - Replay: `./replay.sh crashes/utasker_MODBUS/memory_fill`
* `sf`,`sw`: WriteUnmapped crash at `0x000800d964` (part of `fnWriteInternalFlash` call from `0x800fe52`)
    - Replay: `./replay.sh crashes/utasker_MODBUS/storage_fill`
* `sd`: ReadUnmapped crash as part of `uMemcpy` called at `0x800ffd0`
    - Replay: `./replay.sh crashes/utasker_MODBUS/storage_display`


## uTasker USB

### Out-of-bounds access from interface index in `control_callback`

The firmware fails to validate that the interface index from the setup packet used for device/interface requests are in-bounds.  The interface index (`control_callback::iInterface`) is loaded from `0x20000c78: usb_hardware.ulUSB_buffer + 4`. This buffer is filled as part of `fnExtractFIFO` by reading the USB FIFO data registers: `OTG_FS_DFIFO0`. (Unlike the two false-positives below there doesn't appear to be any hardware checks for the data read from this buffer).

For example when handling a `SET_LINE_CODING`, settings for the target interface are copied into the `uart_setting` array. If the interface number is greater than `USB_CDC_COUNT == 1` then an out-of-bounds write can occur.

Replay: `./replay.sh crashes/utasker_USB/control_callback_bad_interface`


Since this bug corrupts global memory it can crash in many different ways. In the provided replay file, interface value of `0x1c8` causes corruption of the `fnCommandInput::iDebugBufferIndex` (`0x200008a0`) struct at (icount=549495):

```
0x0800f76e: uMemcpy at uTasker/Driver.c:1284
0x08011c3c: control_callback at Applications\uTaskerV1.4/usb_application.c:2426
0x08010066: fnUSB_handle_frame at uTasker/USB_drv.c:1661
0x0800d8e2: fnProcessInput at Hardware\STM32/stm32_USB_OTG.h:324
0x0800db76: USB_OTG_FS_Interrupt at Hardware\STM32/stm32_USB_OTG.h:436
<signal handler called>

=> 0x0800f76e <+10>:    strb.w  r3, [r4], #1        ; r4=0x200008a0
```

Causing a crash the index is later used at (icount=637472):

```
UnhandledException(code=WriteUnmapped, value=0xf20704a0)
0x080135be: fnCommandInput at Applications\uTaskerV1.4/debug.c:8394
0x080122c0: fnApplication at FApplications\uTaskerV1.4/application.c:1213
0x0800ea72: uTaskerSchedule at FuTasker/uTasker.c:401
0x0800c214: main at FHardware\STM32/STM32.c:410
```

### Buffer overflow in `fnExtractFIFO` (false positive)

When handling USB interrupts, the firmware copies data from the USB peripheral's FIFO region to a global buffer (`usb_hardware.ulUSB_buffer`). The length the data to copied is controlled via `usb_hardware.usLength = ((OTG_FS_GRXSTSR << 0x11) >> 0x15)` (this corresponds to the `BCNT: Byte count` field). If this length exceeds the size of the `ulUSB_buffer` then a buffer overflow occurs as part of `fnExtractFIFO` overwriting global variables.

As an example, corruption of `usb_endpoints` occurs at:

```
0x000800d684: fnExtractFIFO at Hardware\STM32\stm32_USB_OTG.h:106.9
0x000800dbd2: fnGetRx at Hardware\STM32\stm32_USB_OTG.h:460.17
0x000800dbd2: USB_OTG_FS_Interrupt at Hardware/STM32/stm32_USB_OTG.h:460
```

Causing a crash at:

```
UnhandledException(code=ReadUnmapped, value=0xffff7f16):
0x000800d796: fnSendUSB_data at Hardware\STM32\stm32_USB_OTG.h:230.5
0x000800fb4a: fnPrepareOutData at uTasker\USB_drv.c:776.5
0x000800f9ca: fnStartUSB_send at uTasker\USB_drv.c:496.9
0x000800f926: entry_usb at uTasker\USB_drv.c:435.17
0x000800f27a: fnWrite at uTasker\Driver.c:303.5
0x0008011af6: fnTaskUSB at Applications\uTaskerV1.4\usb_application.c:1221.13
0x000800ea72: uTaskerSchedule at uTasker\uTasker.c:401.13
0x000800c214: main at Hardware\STM32\STM32.c:410.9
```

According to the MCU spec, the size of the RxFIFO buffer is controlled by `GRXFSIZ.RXFD` which has a minimum value of 16 and a maximum value of 256. `GRXFSIZ.RXFD` is set to 64 at `0x0800de24` (part of `fnConfigUSB`) which corresponds to the amount of memory allocated for `ulUSB_buffer` meaning this bug is a false positive.

Replay: `./replay.sh crashes/utasker_USB/fnExtractFIFO_oob`


## Out-of-bounds access in `fnUSB_handle_frame` from `GRXSTSPR.CHNUM` value (false positive)

When handling a `OTG_FS_GRXSTSR_PKTSTS_OUT_RX` the endpoint/channel number is read from a peripheral: `GRXSTSPR.CHNUM`. The endpoint number is later used as an array index into several global structures without bounds checking. Since the firmware only reserves memory for a single endpoint, endpoint numbers greater than zero can cause the program to crash.

Depending on the specific endpoint value the program can crash in several different ways. For example, in `fnEndpointData`  the endpoint is used to compute the address of a `USB_ENDPOINT` struct allocated for the endpoint. For an endpoint of `5` this pointer points to a region of memory filled with zero. This causes a crash when the firmware attempts to an execute a memcpy operation with a null pointer.

Replay: `./replay.sh crashes/utasker_USB/fnUSB_handle_frame_endpoint`

```
UnhandledException(code=WriteUnmapped, value=0x1000):
0x000800f76e: uMemcpy at uTasker\Driver.c:1284.9
0x000800f400: fnFillBuf at uTasker\Driver.c:485.9
0x0008010174: fnEndpointData at uTasker\USB_drv.c:1897.9
0x000801008a: fnUSB_handle_frame at uTasker\USB_drv.c:1685.9
0x000800d8e2: fnProcessInput at Hardware\STM32\stm32_USB_OTG.h:324.5
0x000800dbda: fnGetRx at Hardware\STM32\stm32_USB_OTG.h:461.17
```

This is a false positive only because only interrupts for endpoint 0 are enabled. `OTG_FS_DOEPINT0` is configured at `0x0800de9c` (part of `fnConfigUSB`).

## Direct manipulation of memory using I/O menu

This bug is similar to the [equivalent bug](#direct-manipulation-of-memory-using-io-menu) in uTasker MODBUS, however the fuzzer must generate an input that does not crash as a result of one of the USB related issues.

Replay: `./replay.sh crashes/utasker_USB/usb_storage_display`

In the provided replay file, the firmware crashes after running a `sd` (Storage display) command.

```
UnhandledException(code=ReadUnmapped, value=0x527e) (icount = 3056270), active_irq = 0
0x000800f76a: uMemcpy at uTasker\Driver.c:1284.9
0x0008012b3a: fnDoCommand at Applications\uTaskerV1.4\debug.c:7807.9
0x0008013668: fnDoDebug at Applications\uTaskerV1.4\debug.c:7914.17
0x00080122e2: fnApplication at Applications\uTaskerV1.4\application.c:1221.17
0x000800ea72: uTaskerSchedule at uTasker\uTasker.c:401.13
0x000800c214: main at Hardware\STM32\STM32.c:410.9
```

## Zephyr SocketCAN

### `canbus` subcommands fail to validate argument count correctly

Given given the command: `canbus attach CAN_1 -` (`-r` and `-e` also crash), the firmware crashes due to a NULL pointer dereference, due to incorrect argument validation.

The firmware implements a "shell" like interface where commands are registered at compile time. The `canbus attach` command handler is defined at: `0x800f82c`:


```rust
shell_static_entry {
    syntax: "attach",
    help: "Attach a message filter and print those messages.\n\
    Usage: attach device_name [-re] id [mask [-r]]\n\
    -r Remote transmission request\n\
    -e Extended address"
    handler: 0x08005af5, // (cmd_attach)
    args: shell_static_args {
        mandatory: 3,
        optional: 3,
    },
}
```

Before executing the `cmd_attach` handler, the firmware validates that at least 3 (and at most 6) arguments are passed to the command. This is done as part of the `cmd_precheck` function (in the compiled version of the code this function has been inlined as part of `execute`).

Due to the way optional arguments are handled in `cmd_attach` this check is insufficient. For `canbus attach CAN_1 -` the code executes `cmd_attach` with:

```c
argc = 3, argv = { [0] = "attach", [1] = "CAN_1", [2] = "-" }
```

`CAN_1` is parsed `device_name` using `z_impl_device_get_binding` and the optional argument `-` is handled as part of `read_frame_options`. The firmware then attempts to parse the `id` argument without checking that there are any entries left in `argv`, this causes a NULL pointer to be passed to `read_id` eventually causing a crash as part of `strtol`.


Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_attach_arg_count`

```
UnhandledException(code=ReadUnmapped, value=0x0) (icount = 392460)
0x0800d2e0: strtol at zephyr/lib/libc/minimal/source/stdlib/strtol.c:58
0x08005e02: read_bitrate at zephyr/drivers/can/can_shell.c:89
0x08005e02: cmd_config at zephyr/drivers/can/can_shell.c:243
0x08001f8c: exec_cmd at zephyr/subsys/shell/shell.c:511
0x080020c0: state_collect at zephyr/subsys/shell/shell.c:909
0x0800bf08: shell_signal_handle at zephyr/subsys/shell/shell.c:1183
0x08002866: shell_thread at zephyr/subsys/shell/shell.c:1235
0x0800b618: z_thread_entry at zephyr/lib/os/thread_entry.c:29
```

The same issue exists other canbus subcommands (`send`, `config`, and `detatch`).

Note: the code in Zephyr containing this bug was rewritten as part of https://github.com/zephyrproject-rtos/zephyr/commit/2f73225aa8d532cd41fd1dcc875d4f189e5db4b0


### Out-of-bounds write in `can_stm32_attach` (false positive)

Setting multiple CAN bus attachments (e.g., using the `canbus attach CAN_1` command) can result in an out-of-bounds write. This OOB write may corrupt global variables causing several different crashes.

However, normally firmware validates that there is a free slot as part of the `can_stm32_set_filter` function (which will return `CAN_NO_FREE_FILTER`). This code involves reading from CAN bus control registers (namely `0x40006604: SOCKET_CAN_1.FM1R` and `0x4000660c: SOCKET_CAN_1.FS1R`). Since the fuzzer generates invalid and inconsistent values for these registers, the check fails making this bug a **false positive**.

Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_attach_fp`

In the replay file the following occurs:


```log
uart:~$ canbus attach CAN_1 0
[00:00:32.314,000] <dbg> Attach filter with ID 0x0 (standard id) and mask 0x7ff  RTR: 0
[00:00:34.619,000] <dbg> can_driver.can_stm32_set_filter: Setting filter ID: 0x0, mask: 0x7ff
[00:00:38.392,000] <dbg> can_driver.can_stm32_set_filter: Filter type: standard ID without mask (1)
[00:00:39.021,000] <dbg> can_driver.can_stm32_set_filter: Filter set! Filter number: 4 (index 2)
Filter ID: 4

uart:~$ canbus attach CAN_1 0
Attach filter with ID 0x0 (standard id) and mask 0x7ff  RTR: 0
[00:02:40.170,000] <dbg> can_driver.can_stm32_set_filter: Setting filter ID: 0x0, mask: 0x7ff
[00:02:45.619,000] <dbg> can_driver.can_stm32_set_filter: Filter type: standard ID without mask (1)
[00:02:51.489,000] <dbg> can_driver.can_stm32_set_filter: Filter set! Filter number: 8 (index 5)
Filter ID: 8
```

Attachments are assigned using:

```c
can_stm32_data* data;
if (filter_nr != CAN_NO_FREE_FILTER) {
    data->rx_cb[filter_index] = cb;
    data->cb_arg[filter_index] = cb_arg;
}
```

The memory reserved for `data->rx_cb/cb_arg` only has capacity for 5 entries, which means a filter_index of 5 is invalid. This causes corruption of `0x20000548: can_stm32_dev_data_1.state_change_isr` at (icount=2720513)

```
0x000080058e6: can_stm32_attach at zephyr/drivers/can/can_stm32.c:966
0x000080058e6: can_stm32_attach_isr at zephyr/drivers/can/can_stm32.c:981
0x00008005c70: cmd_attach at zephyr/drivers/can/can_shell.c:368
0x00008001f8c: exec_cmd at zephyr/subsys/shell/shell.c:511
0x00008001f8c: execute at zephyr/subsys/shell/shell.c:730
0x000080020c0: state_collect at zephyr/subsys/shell/shell.c:909
0x000080020c0: shell_process at zephyr/subsys/shell/shell.c:1341
0x0000800bf08: shell_signal_handle at zephyr/subsys/shell/shell.c:1183
0x00008002866: shell_thread at zephyr/subsys/shell/shell.c:1235
0x0000800b618: z_thread_entry at zephyr/lib/os/thread_entry.c:29
```

This causes a crash when the function is later called:

```
UnhandledException(code=ExecViolation, value=0x20000e18):
0x00020000e18: <invalid>
0x0000800d2a0: can_stm32_bus_state_change_isr at zephyr/drivers/can/can_stm32.c:122
0x0000800d2a0: can_stm32_state_change_isr at zephyr/drivers/can/can_stm32.c:244
0x00008006436: _isr_wrapper at zephyr/arch/arm/core/aarch32/isr_wrapper.S:195
```

A similar issue exists with `canbus detach`:

- Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_detach_fp`


### `net pkt` command dereferences a user provided pointer

The `net pkt` command takes a single argument as an argument and treats it as a pointer (`struct net_pkt *`). Since the pointer is provided by the user, providing an invalid pointer causes the firmware to crash.

Replay: `./replay.sh crashes/Zephyr_SocketCan/net_pkt`


```
uart:~$ net pkt 0x1e
```

Crashes at (icount = 414258):

```
UnhandledException(code=ReadUnmapped, value=0x1e)
0x0008008c4e: net_pkt_buffer_info at zephyr/subsys/net/ip/net_shell.c:3248
0x0008008c4e: cmd_net_pkt at zephyr/subsys/net/ip/net_shell.c:3301
0x0008001f8c: exec_cmd at zephyr/subsys/shell/shell.c:511
0x0008001f8c: execute at zephyr/subsys/shell/shell.c:730
0x00080020c0: state_collect at zephyr/subsys/shell/shell.c:909
0x00080020c0: shell_process at zephyr/subsys/shell/shell.c:1341
0x000800bf08: shell_signal_handle at zephyr/subsys/shell/shell.c:1183
0x0008002866: shell_thread at zephyr/subsys/shell/shell.c:1235
0x000800b618: z_thread_entry at zephyr/lib/os/thread_entry.c:29
```

This appears to be an intentional debugging feature.


### `canbus` subcommands fail to verify device type

The `canbus` subcommands allow the user to specify a target device to execute the commands on. The device is resolved at runtime using the `z_impl_device_get_binding` function. This function can match _any_ device not just canbus devices, and none of the canbus subcommands verify that the target device implements the canbus API. Therefore, if a non-canbus device is specified then a crash will occur when attempting use one of the API functions.

Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_config_stm32_cc`


For example, for the command: `canbus config stm32-cc 0`, `z_impl_device_get_binding` will return: `0x200000c4: __device_rcc_stm32` and the firmware will attempt to execute: `__device_rcc_stm32.driver_api->configure` at `0x08005e28`. `__device_rcc_stm32` does not implement the canbus driver API, so instead calls `stm32_clock_control_on` which treats the second argument as a pointer, causing a crash:

```
UnhandledException(code=ReadUnmapped, value=0x0)
0x000800460c: stm32_exti_unset_callback at zephyr/drivers/interrupt_controller/intc_exti_stm32.c:427.1
0x000800460c: stm32_clock_control_on at zephyr/drivers/clock_control/clock_stm32_ll_common.c:87
0x0008005e2a: z_impl_can_configure at ../include/drivers/can.h:526
0x0008005e2a: can_configure at zephyr/include/generated/syscalls/can.h:75
0x0008005e2a: cmd_config at zephyr/drivers/can/can_shell.c:248
0x0008001f8c: exec_cmd at zephyr/subsys/shell/shell.c:511
0x0008001f8c: execute at zephyr/subsys/shell/shell.c:730
0x00080020c0: state_collect at zephyr/subsys/shell/shell.c:909
0x000800bf08: shell_process at zephyr/subsys/shell/shell.c:1341
0x000800bf08: shell_signal_handle at zephyr/subsys/shell/shell.c:1183
0x0008002866: shell_thread at zephyr/subsys/shell/shell.c:1235
0x000800b618: z_thread_entry at zephyr/lib/os/thread_entry.c:29
```

Similar crashes exist for the other `canbus` subcommands, including:

* `canbus attach`, Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_config_stm32_cc`
* `canbus detach`, Replay: `./replay.sh crashes/Zephyr_SocketCan/canbus_detach_stm32_cc`

### `pwm` subcommands fail to verify device type

Similar to the `canbus` subcommands, the `pwm` subcommands also fail to validate the type of the device that the command is interacting with causing a crash when the API is called.

Replay: `./replay.sh crashes/Zephyr_SocketCan/pwm_cycles`

For example, the command: `pwm cycles UART_2 ...` crashes at:

```
UnhandledException(code=WriteUnmapped, value=0x0)
0x000800e4b4: LL_USART_ReceiveData8 at modules/hal/stm32/stm32cube/stm32l4xx/drivers/include/stm32l4xx_ll_usart.h:4523
0x000800e4b4: uart_stm32_poll_in at zephyr/drivers/serial/uart_stm32.c:394
0x00080093c8: z_impl_pwm_pin_set_cycles at ../include/drivers/pwm.h:87
0x00080093c8: pwm_pin_set_cycles at zephyr/include/generated/syscalls/pwm.h:33
0x00080093c8: cmd_cycles at zephyr/drivers/pwm/pwm_shell.c:55
0x0008001f8c: exec_cmd at zephyr/subsys/shell/shell.c:511
0x0008001f8c: execute at zephyr/subsys/shell/shell.c:730
0x00080020c0: state_collect at zephyr/subsys/shell/shell.c:909
0x00080020c0: shell_process at zephyr/subsys/shell/shell.c:1341
0x000800bf08: shell_signal_handle at zephyr/subsys/shell/shell.c:1183
0x0008002866: shell_thread at zephyr/subsys/shell/shell.c:1235
0x000800b618: z_thread_entry at zephyr/lib/os/thread_entry.c:29
```


The other pwm subcommands also contain the same issue (i.e., `pwm usec` and `pwm nsec`).

(See: https://github.com/zephyrproject-rtos/zephyr/blob/60a20471b561a0a3a74b377991fe1976c2ea83d7/drivers/pwm/pwm_shell.c)


