------------------------------ MODULE position ------------------------------
EXTENDS Integers, TLC

VARIABLES qty, entry_price

Init ==
    /\ qty = 0
    /\ entry_price = 0

Increase(q, price) ==
    /\ qty' = qty + q
    /\ entry_price' = (entry_price * qty + price * q) \div (qty + q)
    

Invariant ==
    /\ qty >= 0
    /\ entry_price >= 0
 
 Next == 
    \A quantity \in (1..100): Increase(quantity, 100)
 
 THEOREM Spec == Init /\ [][Next]_qty
 
=============================================================================
\* Modification History
\* Last modified Sun Jun 01 16:12:07 CEST 2025 by magewe
\* Created Sun Jun 01 14:52:16 CEST 2025 by magewe
