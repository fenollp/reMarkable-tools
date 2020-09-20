# Whiteboard hypercard

https://www.reddit.com/r/RemarkableTablet/comments/ioa37o/realtime_collaboration_drawing_or_whiteboarding/

Real-time collaboration, drawing or whiteboarding

draw/present/convey information in a realtime collab session, as well as taking notes

multiple people can draw and collab on like a virtual whiteboard, but failing that present in realtime with one

## Hack

1. Run `make debug` on a machine on the same network as your reMarkable tablet.
1. Listen to a room's events with
```
grpcurl -proto marauder/proto/hypercard/whiteboard.proto \
  -rpc-header 'x-user: moi1' \
  -d '{"room_id":"bla" }' \
  -plaintext localhost:10000 \
  hypercard.whiteboard.Whiteboard/RecvEvents
```
1. In another shell, send some event: (it appears on the previous shell)
```
grpcurl -proto marauder/proto/hypercard/whiteboard.proto \
  -rpc-header 'x-user: me2' \
  -d '{"room_ids":["bla","bloop"] ,"event":{"event_drawing":{}} }' \
  -plaintext localhost:10000 \
  hypercard.whiteboard.Whiteboard/SendEvent
```
1. Have a second user listen to the same room and observe the first user getting a connection event:
```
grpcurl -proto marauder/proto/hypercard/whiteboard.proto \
  -rpc-header 'x-user: me2' \
  -d '{"room_id":"bla" }' \
  -plaintext localhost:10000 \
  hypercard.whiteboard.Whiteboard/RecvEvents
```

.
.
.
1. Install the client & tune room ID and server IP.
