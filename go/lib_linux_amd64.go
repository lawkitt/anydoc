//go:build linux && amd64 && cgo

package anydoc

/*
#cgo LDFLAGS: -L${SRCDIR}/../target/release -lanydoc_go -lm -lstdc++ -ldl -lpthread
*/
import "C"
