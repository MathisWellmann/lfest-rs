---------------------------- MODULE order_margin ----------------------------

EXTENDS Naturals, Integers, FiniteSets, TLC

CONSTANTS Orders

VARIABLES
    balances, \* contains available, position_margin and order_margin
    position, \* contains the quantity and entry_price
    bids,      \* User buy orders
    asks      \* User sell orders

\* Initial State
Init ==
    /\ balances = [ available |-> 1000, position_margin |-> 0, order_margin |-> 0 ]
    /\ position = [ qty |-> 0, entry_price |-> 0 ]
    /\ bids = {}
    /\ asks = {}

AddBid(order) ==
    /\ order.qty > 0
    /\ order.limit_price > 0
    /\ balances' = [ balances EXCEPT !.available = @ - order.qty * order.limit_price, !.order_margin = 0 ]

\* Next-state relation
Next ==
    \/ \A order \in Orders:
        AddBid(order)

\* Specification: Initial condition and all possible transitions
Spec == Init /\ [][Next]_<<>>

=============================================================================
\* Modification History
\* Last modified Fri May 30 19:00:24 CEST 2025 by magewe
\* Created Fri May 30 15:56:29 CEST 2025 by magewe
