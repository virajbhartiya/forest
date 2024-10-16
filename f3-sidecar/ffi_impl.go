package main

import (
	"context"
	"os"

	logging "github.com/ipfs/go-log/v2"
)

func init() {
	setGoDebugEnv()
	logging.SetAllLoggers(logging.LevelInfo)
	err := logging.SetLogLevel("dht", "error")
	checkError(err)
	err = logging.SetLogLevel("dht/RtRefreshManager", "warn")
	checkError(err)
	err = logging.SetLogLevel("net/identify", "error")
	checkError(err)
	err = logging.SetLogLevel("f3/sidecar", "debug")
	checkError(err)
	GoF3NodeImpl = &f3Impl{ctx: context.Background()}
}

type f3Impl struct {
	ctx context.Context
}

func (f3 *f3Impl) run(rpc_endpoint string, f3_rpc_endpoint string, initial_power_table string, bootstrap_epoch int64, finality int64, db string, manifest_server string) bool {
	err := run(f3.ctx, rpc_endpoint, f3_rpc_endpoint, initial_power_table, bootstrap_epoch, finality, db, manifest_server)
	return err == nil
}

func checkError(err error) {
	if err != nil {
		panic(err)
	}
}

// To avoid potential panics
// See <https://github.com/ChainSafe/forest/pull/4636#issuecomment-2306500753>
func setGoDebugEnv() {
	os.Setenv("GODEBUG", "invalidptr=0,cgocheck=0")
}
