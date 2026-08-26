# Membership Intelligence

This context defines the language used to describe household membership, participation,
retention, and departure at Temple Emanu-El.

## Language

**Membership Household**:
The Account-level unit that holds a family's membership relationship. It may contain one or more people, but membership analysis treats the household as one unit.
_Avoid_: Member, person, customer

**Membership Spell**:
One continuous period in which a Membership Household is considered active, bounded by joining and resignation events. A rejoined household has more than one spell.
_Avoid_: Membership record, tenure

**Entry Job**:
A stated reason a household joined, such as school, worship, family, clergy, or community. Entry Jobs are multi-label and are evidence of motivation, not proof of a household's complete intent.
_Avoid_: Join channel, acquisition source

**Relationship Anchor**:
Observed participation that connects a household to the congregation, such as school enrollment, dues renewal, or committee service.
_Avoid_: Engagement reason, activity

**Exit Outcome**:
The multi-label classification of why a Membership Spell ended. Raw resignation reasons remain available even when a primary reporting category is assigned.
_Avoid_: Churn reason

**Addressable Churn**:
An Exit Outcome plausibly related to engagement, affordability, payment, dissatisfaction, or choosing another congregation.
_Avoid_: All resignations, preventable churn

**Structural Exit**:
An Exit Outcome caused by moving, death, or age and illness, where retention intervention is generally inappropriate.
_Avoid_: Churn

**Conversion Loss**:
An Exit Outcome in which a household leaves an introductory or age-limited membership tier without converting to an ongoing tier.
_Avoid_: Structural Exit, Addressable Churn

**Administrative or Unknown Exit**:
An Exit Outcome that cannot be classified reliably because its reason is administrative, uncoded, or ambiguous.
_Avoid_: Addressable Churn

**Renewal Evidence**:
A membership-dues billing record that supports, but does not by itself determine, whether a household renewed.
_Avoid_: Membership truth, payment status

**Risk Evidence**:
A current or recent observed condition associated with Addressable Churn. A historical fact by itself, such as school ending many years ago, is not current Risk Evidence.
_Avoid_: Churn reason, risk label

**Watch List**:
Current Membership Households selected for staff review by a validated model and at least two independent classes of recent Risk Evidence.
_Avoid_: Churn list, predicted resignations
