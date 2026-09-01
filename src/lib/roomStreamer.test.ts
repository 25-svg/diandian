import assert from "node:assert/strict";
import {
  normalizeRoomStreamerName,
  roomStreamerLabel,
  validateRoomStreamerName,
} from "./roomStreamer.js";

assert.equal(normalizeRoomStreamerName(" 主播：罗雨欣 "), "罗雨欣");
assert.equal(validateRoomStreamerName("罗雨欣"), "");
assert.match(validateRoomStreamerName("罗雨欣，侯梦娜"), /不能包含标点/);
assert.match(validateRoomStreamerName("这是一个超过十二个字符的主播名称"), /1 至 12/);
assert.equal(roomStreamerLabel(""), "待指定");

console.log("roomStreamer model tests passed");
