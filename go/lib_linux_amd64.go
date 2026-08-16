//go:build linux && amd64 && cgo

package anydoc

/*
#cgo LDFLAGS: -lanydoc_go -lm -lstdc++ -ldl -lpthread
*/
import "C"
