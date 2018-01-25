#!/bin/bash

set -e

cargo build
program=target/debug/influence

TMPDIR=/tmp/influence-dpi-test
mkdir -p $TMPDIR
rm -f $TMPDIR/* # not recursive! we're not valve!

main() {
    Xvfb :1 -screen 0 1920x1080x24 &

                  #  scale  dpi_scale 
    make_screenshot  1      1.0
    make_screenshot  1      1.5
    # make_screenshot  2      1.0
    # make_screenshot  1      1.0
    # make_screenshot  1      1.5
    # make_screenshot  2      1.0
    # make_screenshot  2      1.5
    # make_screenshot  3      1.0

    kill %1

    images=()
    for dpi in $(cat $TMPDIR/dpi-list.txt); do
        images+=( $TMPDIR/dpi-$dpi.png )
    done

    montage ${images[@]} -geometry 750x375+0+0 -tile 2x -background '#1d1f21' $TMPDIR/montage.png
}

make_screenshot() {
    scale=$1
    dpi_scale=$2
    dpi=$3

    sleep 0.4s
    DISPLAY=:1 GDK_SCALE=$scale GDK_DPI_SCALE=$dpi_scale $program &

    sleep 0.4s
    geom="$(DISPLAY=:1 xwininfo -root -tree | grep influence | grep -oP '\d+x\d+\+\d+\+\d+')"
    DISPLAY=:1 maim -g "$geom" $TMPDIR/dpi-$scale-$dpi_scale.png

    echo -n "$scale-$dpi_scale " >> $TMPDIR/dpi-list.txt

    kill %2
    sleep 0.5s
}

main
