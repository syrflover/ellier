FROM alpine:edge

RUN apk add ffmpeg~=6.1.1
RUN apk add streamlink~=6.6.2
RUN apk add mkvtoolnix~=82.0

ARG BINARY_FILE

COPY $BINARY_FILE /erish-encoder

ENTRYPOINT [ "/erish-encoder" ]
