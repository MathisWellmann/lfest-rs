--------------------------- MODULE active_orders ---------------------------

EXTENDS Integers, TLC, Seq

CONSTANT side == { Buy, Sell };
VARIABLES book

(* Initial state: no active orders *)
Init == book = [bids |-> <<>>, asks |-> <<>> ]

PlaceOrder(qty, limit_price, side) == 
    /\ qty > 0
    /\ limit_price > 0
    /\ if side = Buy { 
        book' = [ ]
       } else { }
=============================================================================
\* Modification History
\* Last modified Fri May 30 11:37:12 CEST 2025 by magewe
\* Created Fri May 30 11:30:06 CEST 2025 by magewe
