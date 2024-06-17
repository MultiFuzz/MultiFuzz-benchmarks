set width 0
set height 0
set verbose off

set pagination off

file ./benchmarks/MultiFuzz/riot-ccn-lite-relay/ccn-lite-relay.elf
target remote 127.0.0.1:9999

monitor goto 1

break _now_next
commands 1 
    silent
    printf "_now_next(0x%x)\n", $r0
    print *clock
    printf "\n"
    continue
end

break *0x154f4
commands 2 
    silent
    printf "ztimer_handler::entry->callback(0x%x)\n", $r0
    print *((ztimer_t*)$r0)
    printf "\n"
    continue
end

break *_evtimer_handler
commands 3
    silent
    printf "_evtimer_handler(0x%x)\n", $r0
    print *((evtimer_t*)$r0)
    print *((evtimer_event_t *)((evtimer_t*)$r0)->events)
    printf "\n"
    continue
end

break *0x168f6
commands 4
    silent
    printf "\n\n__wrap_free(ptr=0x%x)\n", $r0
    print *((struct ccnl_face_s *)$r0)
    printf "\n"
    continue
end

break *evtimer_add
commands 5
    silent
    # evtimer_add(evtimer_t *evtimer,evtimer_event_t *event)
    printf "evtimer_add(0x%x,0x%x)\n", $r0, $r1
    print *evtimer
    print *event
    printf "\n"
    continue
end

break *evtimer_del
commands 6
    silent
    # evtimer_del(evtimer_t *evtimer,evtimer_event_t *event)
    printf "evtimer_del(0x%x,0x%x)\n", $r0, $r1
    print *evtimer
    print *event
    printf "\n"
    continue
end

break *msg_send_int
commands 7
    silent
    # msg_send_int(msg_t *m,kernel_pid_t target_pid)
    printf "msg_send_int(0x%x,0x%x)\n", $r0, $r1
    printf "\n"
    continue
end

continue
