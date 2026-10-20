import * as React from "react";

// Heading outline: each titled surface says which level the next heading inside it should take, so section
// titles follow the page or dialog title without skipping a level.
// 标题大纲：每个带标题的界面声明其内部下一个标题应使用的层级，使区块标题紧随页面或对话框标题而不跳级。

// HeadingLevel is the range of section heading levels the kit renders.
// HeadingLevel 是套件渲染的区块标题层级范围。
export type HeadingLevel = 2 | 3 | 4 | 5 | 6;

// HeadingLevelContext holds the level for the next nested heading; a page's h1 makes it 2.
// HeadingLevelContext 保存下一个嵌套标题的层级；页面的 h1 使其为 2。
export const HeadingLevelContext = React.createContext<HeadingLevel>(2);

// useHeadingLevel reads the level the next heading should take.
// useHeadingLevel 读取下一个标题应使用的层级。
export function useHeadingLevel(): HeadingLevel {
  return React.useContext(HeadingLevelContext);
}

// nestedHeadingLevel is the level for headings below one at `level`, held at h6.
// nestedHeadingLevel 是位于 `level` 标题之下的层级，最多为 h6。
export function nestedHeadingLevel(level: HeadingLevel): HeadingLevel {
  return Math.min(level + 1, 6) as HeadingLevel;
}
