# Straw man principles

* Don't modify P4 or DDlog at all.  Use the languages as is.

* Write a p42ddlog program, analogous to ovsdb2ddlog, whose input is a
  .p4 file and whose output is a .dl file that declares an output
  relation for each P4 table.  Maybe it declares an enum for each
  table's set of actions, too (see "actions vs values" below).

* The DDlog program would have input relations populated by the
  management plane.  Productions map them down into the output
  relations that feed into P4.

* The P4 program needs to be able provide feedback to the DDlog
  control plane, the equivalent of "send to cpu" etc.  Approaches:

  - P4 program can have externs written in Rust that do arbitrary
    things.

  - P4 program can add to relations that the DDlog program uses as
    input, e.g. P4 outputs to them, to DDlog they are input relations.
    (This might just be a special case of externs written in Rust.)

# DDlog/P4 interface

We need to define data structure to cross the language boundary.
DDlog output relations need to be in a form that works for P4.  Data
passed by the P4 program to Rust externs needs to be in a form that
works for Rust/DDlog.

## DDlog relation <-> P4 table

DDlog relations and P4 tables have some impedance mismatches.
https://p4.org/p4-spec/docs/P4-16-v1.2.1.html#sec-mau-semantics has
good details.

### naming

Tables in P4 do not specify column names, but rather arbitrary
expressions used for lookup, e.g.:

>         y & 0x7  : exact;
>         f1(x, y) : exact;
>         y        : exact;

DDlog needs column names.  This is easy enough because P4 has a
standard @name annotation for that, and the compiler generates one if
it's missing:

>     key = {
>         y & 0x7  : exact @name("masked_y");
>         f1(x, y) : exact @name("f1");
>         y        : exact;     //  defaults to @name("y").
>     }

### actions vs values

Tables in P4 do not specify a value type but rather the
possibly-parameterized classes of side effects that may take place as
a result of lookup (actions), e.g.

>          actions = {
>               Drop_action;
>               Set_nhop;
>          }

where:

>       action Drop_action()
>       { outCtrl.outputPort = DROP_PORT; }

>       action Set_nhop(IPv4Address ipv4_dest, PortId port) {
>           nextHop = ipv4_dest;
>           headers.ip.ttl = headers.ip.ttl-1;
>           outCtrl.outputPort = port;
>       }

One could translate P4 actions to DDlog in a graceful way using an
enum.

### default action

A P4 table may have a default action.  The default action is executed
when the table is invoked and there is no match.

This is mostly transparent to DDlog (but unless the P4 declares it
const), the default action is really just the *default* default
action: the control plane is allowed to change it at runtime, so DDlog
should probably be able to do it too.  I guess that could be through
an output relation somehow?

The eBPF compiler models each P4 table as two tables: the normal table
and a second table that only contains the default action.

### match interface

P4 tables have an interface that is uniform from P4's data plane point
of view (it's just a lookup of a key).  From DDlog there's more
diversity based on the P4 match_kind:

    * exact: Nothing extra, although it might be good to be able to
      specify array vs. hash table representation.

    * ternary: Each key field needs a corresponding mask field.  The
      table as a whole needs a priority field.

    * lpm: Each key field needs a corresponding prefix length field.
      The prefix length also serves as the priority.  (Does it make
      sense to have more than one lpm field in a table or to have them
      coexist with ternary fields?  I don't see how to make sense of
      that.)

Other match kinds could be supported as extensions:

    * prefix: Each key field needs a correponding prefix length.  The
      table as a whole needs a priority.  There is no implication that
      the priority is the prefix length.

    * range: Range match.  I'm not sure whether P4 supports this
      gracefully; maybe it would have to be two separate match kinds
      range_low and range_high.

Architectures may define new `match_kinds`.  Google has proposed an
additional `match_kind` `optional`.

# DDlog <-> P4 types

P4 and DDlog have similar bit<N> types.

P4 int<N> is similar to DDlog signed<N>.

Rust doesn't have anything like bit<N> or signed<N> except for N =
{8,16,32,64,128}.  Rust doesn't have bit-fields in structs. See
https://immunant.com/blog/2020/01/bitfields/ for an overview of the
state of the art of Rust bit-field substitutes.  None of them looks
great.

###

How do we work with nondeterministic systems?  Like NSX's boolean
minimization system?  Or allocators?

1. Bottom-up: start from unmodified P4 program, stack a DDlog-based
   control plane/controller on top of it

   - p4toddlog (or write it by hand)
   - P4 Runtime glue (Antonin built P4 Runtime)
     * grpc crate

2. P4+: replace actions with values

What about distribution across multiple switches?  Families of similar
programs (multiple hypervisors).  Different families (HV vs gateway vs
pool).

* Global tables.

* Instance tables.

Write a paper about the new OVN.
- Benchmarking.
- Plot numbers with Hillview.
- Performance, expressivity.

Broadcom table-based API for their switches (Vladimir worked on it)
"Logical Table API" SDKLT https://github.com/Broadcom-Network-Switching-Software/SDKLT
