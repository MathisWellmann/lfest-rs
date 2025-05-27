------------------------------- MODULE lfest -------------------------------
EXTENDS Integers, TLC

VARIABLES balances

(* Define the state as a record with three balances *)
(*state == [available: INT, positionMargin: INT, orderMargin: INT]*)

(* Initial state: no position active, so only available balance *)
Init == balances = [available |-> 1000, positionMargin |-> 0, orderMargin |-> 0]

(* Place an order: requires sufficient available balance *)
PlaceOrder(amount) == 
    /\ amount > 0
    /\ balances.available >= amount
    /\ balances' = [balances EXCEPT !.available = balances.available - amount, !.orderMargin = balances.orderMargin + amount]

(* Fill an order: move margin from orderMargin to positionMargin *)
FillOrder(amount) == 
    /\ amount > 0
    /\ balances.orderMargin >= amount
    /\ balances' = [balances EXCEPT !.orderMargin = balances.orderMargin - amount, !.positionMargin = balances.positionMargin + amount]

(* Close a position: move margin from positionMargin to available balance *)
ClosePosition(amount) == 
    /\ amount > 0
    /\ balances.positionMargin >= amount
    /\ balances' = [balances EXCEPT !.positionMargin = balances.positionMargin - amount, !.available = balances.available + amount]

(* Define the next-state relation *)
Next == 
    \A amount \in 0..1000 => PlaceOrder(amount) \/ FillOrder(amount) \/ ClosePosition(amount)

(* Safety: Ensure all balances are non-negat ive *)
Safety == balances.available >= 0 /\ balances.positionMargin >= 0 /\ balances.orderMargin >= 0

(* The specification is the combination of the initial state and transitions *)
Spec == Init /\ [][Next]_balances /\ Safety
=============================================================================
\* Modification History
\* Last modified Tue May 27 16:25:31 CEST 2025 by magewe
\* Created Tue May 27 13:42:49 CEST 2025 by magewe
    