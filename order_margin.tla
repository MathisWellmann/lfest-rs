---------------------------- MODULE order_margin ----------------------------

EXTENDS Naturals, Integers, FiniteSets, TLC

\* Constants
CONSTANTS Orders, MaxMargin, NullUser

\* Variables
VARIABLES
    orders,          \* Map from Orders to [user, price, quantity, margin]
    activeOrders,    \* Set of currently active order IDs
    userMargin       \* Map from Users to [total, locked, position]

\* Initial State
Init ==
    /\ orders = [o \in Orders |-> [user |-> NullUser, price |-> 0, quantity |-> 0, margin |-> 0]]
    /\ activeOrders = {}
    /\ userMargin = [u \in Users |-> [total |-> MaxMargin, locked |-> 0, position |-> 0]]

\* Action: Add a new order
AddOrder(user, order_id, margin) ==
    /\ order_id \in Orders
    /\ order_id \notin activeOrders
    /\ margin > 0
    /\ userMargin[user].total - userMargin[user].locked >= margin
    /\ orders' = [orders EXCEPT ![order_id] = [user |-> user, price |-> 0, quantity |-> 0, margin |-> margin]]
    /\ activeOrders' = activeOrders \cup {order_id}
    /\ userMargin' = [userMargin EXCEPT ![user].locked = userMargin[user].locked + margin]
    /\ UNCHANGED <<userMargin[user].total, userMargin[user].position>>

\* Next-state relation
Next ==
    \/ \E user \in Users, order_id \in Orders, margin \in 1..MaxMargin:
        AddOrder(user, order_id, margin)


\* Specification: Initial condition and all possible transitions
Spec == Init /\ [][Next]_<<orders, activeOrders, userMargin>>
\* Optional: Add fairness conditions for liveness if needed
\* FairSpec == Spec /\ WF_vars(Next)

\* Invariant: Locked margin never exceeds total margin
Inv == \A u \in Users: userMargin[u].locked <= userMargin[u].total

=============================================================================
\* Modification History
\* Last modified Fri May 30 19:00:24 CEST 2025 by magewe
\* Created Fri May 30 15:56:29 CEST 2025 by magewe
