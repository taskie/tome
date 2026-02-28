import { IchStat } from "@/api/types";
import StatGroupItem from "./StatGroupItem";

type Props = {
  workspaceName: string;
  groupName?: string;
  stats: IchStat[];
  mode?: string;
  diffSource?: { groupName: string; pathPrefix: string };
};

export const StatGroup: React.FC<Props> = ({ workspaceName, groupName, stats, mode, diffSource }) => {
  return (
    <ul>
      {stats.map((s) => (
        <StatGroupItem
          key={s.id}
          workspaceName={workspaceName}
          groupName={groupName}
          stat={s}
          mode={mode}
          diffSource={diffSource}
        />
      ))}
    </ul>
  );
};

export default StatGroup;
