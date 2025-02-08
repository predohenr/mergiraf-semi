import (
	"fmt"
	"net"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/redis/go-redis/v9"
)

// RedisClientFromURL creates a new Redis client based on the provided URL.
// The URL scheme can be either `redis` or `redis+sentinel`.
func RedisClientFromURL(url string) (redis.UniversalClient, error) {
	if url == "" {
		return nil, nil
	}
	redisOptions, err := redis.ParseURL(url)
	if err != nil {
		return nil, err
	}
	return redis.NewClient(redisOptions), nil
}
