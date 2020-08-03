Dataplane tables have an interface that is uniform from a lookup point
of view (accepting just a key, which is often a field from the packet)
but diverse from the insertion point of view.  We could support these
in DDDP with some extra keywords that go inside match() in the
position where "primary key" normally goes.  The following could
appear any number of times.  Each <field> is the name of a table
column:

    * exact(<field>): exact match on specified fields

    * prefix(<field>, <plen>): prefix match on pair of columns

    * ternary(<field>, <mask>): ternary match on pair of columns

    * range(<lower>, <upper>): range match (inclusive or exclusive?)

These could appear at most once:

    * priority(<priority>): tie-breaker for handling multiple matches;
      arbitrary choice if there are multiple highest priority matches

    * lpm(<field>, <plen>): short for prefix(<field>, <plen>),
      priority(<field>, <plen>)

Questions:

    * How do we handle matches with "prefix" or "ternary" but without
      "priority"?  They could yield a set or just pick an arbitrary.

    * Does a primary key imply an exact-match on that field?

    * Allow multiple match declarations on a relation? (What syntax to
      query them?)