import { IchStat } from "@/api/types";
import GroupLink from "./GroupLink";
import StatLink from "./StatLink";
import FootprintLink from "./FootprintLink";

type Props = {
  workspaceName: string;
  groupName?: string;
  stat: IchStat;
};

export const StatGroupItem: React.FC<Props> = ({
  workspaceName,
  groupName,
  stat: { path, group_name, version, mtime, size, digest, updated_at },
}) => {
  return (
    <li>
      {digest != null ? <FootprintLink workspaceName={workspaceName} digest={digest} length={8} /> : undefined}
      {" / "}
      {mtime != null ? mtime : "Nothing"}
      {" / "}
      {updated_at}
      {" / "}
      <GroupLink workspaceName={workspaceName} groupName={group_name ?? groupName ?? "default"} />
      {" / "}
      <StatLink workspaceName={workspaceName} groupName={group_name ?? groupName ?? "default"} path={path} />
      {" / "}
      {version}
      {" / "}
      {size != null ? size : "Nothing"}
    </li>
  );
};

export default StatGroupItem;
