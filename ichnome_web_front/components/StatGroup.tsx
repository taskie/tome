import { IchStat } from "@/api/types";
import StatGroupItem from "./StatGroupItem";

type Props = {
  workspaceName: string;
  groupName?: string;
  stats: IchStat[];
};

export const StatGroup: React.FC<Props> = ({ workspaceName, groupName, stats }) => {
  return (
    <ul>
      {stats.map((s) => (
        <StatGroupItem key={s.id} workspaceName={workspaceName} groupName={groupName} stat={s} />
      ))}
    </ul>
  );
};

export default StatGroup;
