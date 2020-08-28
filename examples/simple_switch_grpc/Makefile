p4_14 = packet_io.p4 loopback.p4
p4_16 = clone.p4 digest.p4 meter.p4 optional.p4 ternary.p4 counter.p4	\
	multicast.p4
p4 = $(p4_14) $(p4_16)

outputs = $(p4:.p4=.json) $(p4:.p4=.p4i) $(p4:.p4=.p4info.txt) $(p4:.p4=.p4info.bin)

all: $(outputs)

%.json %.p4i %.p4info.txt %.p4info.bin: %.p4
	p4c --std $(if $(filter $<,$(p4_14)),p4-14,p4-16) --p4runtime-files $(<:.p4=.p4info.txt),$(<:.p4=.p4info.bin) $<

clean:
	rm -f $(outputs)
