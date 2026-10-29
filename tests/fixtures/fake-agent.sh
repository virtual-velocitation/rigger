#!/bin/sh
# A record id belongs to the unit and role that recorded it, so each spawn names its decision
# after the spawn it runs as (`RIGGER_SPAWN`); a spawn with no id records plain `sd1`.
echo 'starting work, not json'
echo "{\"type\":\"DecisionMade\",\"data\":{\"id\":\"sd1${RIGGER_SPAWN:+-$RIGGER_SPAWN}\",\"summary\":\"from a real subprocess\"}}"
echo '{"type":"UnitProposed","data":{"id":"su1","coverage":"c"}}'
echo '{"id":"final","pass":true}'
